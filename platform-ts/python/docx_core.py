"""The shared half of the document worker (contract version 1 and 2): job loading, rendering, logging, fault injection.

`docx_worker.py` runs one job per process; `docx_server.py` runs many jobs in one long-lived process. Both call `render`.

Output formats (job field `format`, default `docx`):
    docx  the template-free generator (docxgen/gen_docx.py, stdlib only)
    html  one self-contained HTML file: the core's parts in a document skeleton with the same CSS the PDF uses
    pdf   the same HTML through WeasyPrint (optional dependency; the engine is one function, `pdf_bytes`, so Prince can replace it)

Logging: one JSON line per event on stderr, `{"level","msg","req_id","job",...}`; the shell reads stderr and re-emits each line
in its own log with the same request id, so one request is greppable from the HTTP request to this process.
mypy --strict clean, no typing.Any.
"""

from __future__ import annotations

import html
import importlib
import json
import os
import platform
import resource
import shutil
import signal
import subprocess
import sys
import tempfile
import threading
import time
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path
from typing import Protocol

import jinja2

sys.path.insert(0, str(Path(__file__).parent / "docxgen"))
import gen_docx  # noqa: E402
from profile_look import LOOK  # noqa: E402
from style_table import css as style_css  # noqa: E402

Scalar = str | int | float | bool | None


class JobError(Exception):
    def __init__(self, kind: str, message: str) -> None:
        super().__init__(message)
        self.kind = kind


@dataclass(frozen=True)
class Job:
    request_key: str
    request_id: str
    job_id: str
    parts: list[dict[str, object]]
    book: str
    title1: str
    title2: str
    output: Path
    fmt: str
    #: `doc`: DOC's own HTML (its header template and CSS, its markup in the parts); `proposed`: the proposed look's skeleton and CSS
    look: str = "proposed"
    #: DOC's `assembly_layout_kind` (`1c`, `1c_c`, `2c_sl_sr`, `2c_sl_sr_c`): the compact layouts use the compact header template
    layout: str = "1c"
    #: PDF engine for this job: `prince`, `weasyprint`; `auto` picks Prince when the request asks for it (`use_prince`) and the binary is installed
    pdf_engine: str = "weasyprint"
    use_prince: bool = False


def log(level: str, msg: str, **fields: Scalar) -> None:
    """One JSON log line on stderr (never stdout: the persistent server's stdout is its protocol channel)."""
    record: dict[str, Scalar] = {"level": level, "msg": msg, "ts": round(time.time(), 3), "pid": os.getpid()}
    record.update(fields)
    sys.stderr.write(json.dumps(record) + "\n")
    sys.stderr.flush()


def rss_kb() -> int:
    """Current (not peak) resident set in kB, from /proc/self/statm; 0 if unavailable."""
    try:
        pages = int(Path("/proc/self/statm").read_text().split()[1])
        return pages * os.sysconf("SC_PAGE_SIZE") // 1024
    except (OSError, ValueError, IndexError):
        return 0


def watch_parent() -> None:
    """Exit if the shell that started this process is gone (SIGKILLed, OOM-killed): an orphaned worker must not outlive its parent.

    The parent is the pid the shell passed in DOCX_PARENT_PID, not `getppid()` at the time this runs: the imports before this point
    take ~100 ms, a parent killed in that window would already be gone and `getppid()` would then name its replacement.
    """
    parent = int(os.environ.get("DOCX_PARENT_PID", "0")) or os.getppid()
    if os.getppid() != parent:
        os._exit(4)

    def loop() -> None:
        while True:
            time.sleep(0.5)
            if os.getppid() != parent:
                os._exit(4)

    threading.Thread(target=loop, daemon=True).start()


def fault() -> None:
    """Test-only fault injection from DOCX_WORKER_FAULT. The `_once:<path>` forms fire only while the marker file does not exist."""
    spec = os.environ.get("DOCX_WORKER_FAULT", "")
    if "_once:" in spec:
        name, _, marker = spec.partition(":")
        if Path(marker).exists():
            return
        Path(marker).write_text("fired")
        spec = name.removesuffix("_once")
    if spec == "hang":
        signal.signal(signal.SIGTERM, signal.SIG_IGN)  # a hung worker that ignores polite requests
        while True:
            time.sleep(3600)
    elif spec == "crash":
        os.kill(os.getpid(), signal.SIGSEGV)
    elif spec == "garbage":
        sys.stdout.write("not json at all\n")
        sys.stdout.flush()
        raise SystemExit(0)
    elif spec.startswith("slow:"):
        time.sleep(int(spec[5:]) / 1000)
    elif spec.startswith("memhog:"):
        hog = bytearray(int(spec[7:]) * 1024 * 1024)
        hog[::4096] = b"x" * len(hog[::4096])
        time.sleep(2)  # hold it long enough for the supervisor's memory check to see it


def str_field(job: dict[str, object], name: str) -> str:
    value = job.get(name)
    if not isinstance(value, str):
        raise JobError("input", f"{name} must be a string")
    return value


def load_job(path: str) -> Job:
    try:
        raw = json.loads(Path(path).read_text(encoding="utf-8"))
    except (OSError, ValueError) as e:
        raise JobError("input", f"cannot read job file: {e}") from e
    if not isinstance(raw, dict) or raw.get("version") not in (1, 2):
        raise JobError("input", "unsupported DocxJob version")
    parts_raw = raw.get("parts")
    if not isinstance(parts_raw, list) or not all(isinstance(p, dict) for p in parts_raw):
        raise JobError("input", "parts must be a list of objects")
    parts: list[dict[str, object]] = [p for p in parts_raw if isinstance(p, dict)]
    fmt = raw.get("format", "docx")
    if fmt not in ("docx", "html", "pdf"):
        raise JobError("input", f"unknown format {fmt!r}")
    request_id = raw.get("request_id", "")
    look = raw.get("look", "proposed")
    layout = raw.get("layout", "1c")
    pdf_engine = raw.get("pdf_engine", "weasyprint")
    if look not in ("doc", "proposed") or pdf_engine not in ("auto", "prince", "weasyprint") or not isinstance(layout, str):
        raise JobError("input", "look, layout or pdf_engine has an unknown value")
    return Job(
        request_key=str_field(raw, "request_key"),
        request_id=request_id if isinstance(request_id, str) else "",
        job_id=str_field(raw, "job_id"),
        parts=parts,
        book=str_field(raw, "book"),
        title1=str_field(raw, "title1"),
        title2=str_field(raw, "title2"),
        output=Path(str_field(raw, "output_path")),
        fmt=str(fmt),
        look=str(look),
        layout=layout,
        pdf_engine=str(pdf_engine),
        use_prince=raw.get("use_prince") is True,
    )


TEMPLATES_DIR = Path(os.environ.get("DOC_TEMPLATES_DIR", str(Path(__file__).parent / "templates")))
_TEMPLATES = jinja2.Environment(loader=jinja2.FileSystemLoader(str(TEMPLATES_DIR)), autoescape=False)  # noqa: S701 (DOC's own setting: its templates take already-escaped titles)
_COMPACT_LAYOUTS = ("1c_c", "2c_sl_sr_c")


def part_content(job: Job) -> str:
    """DOC's `compose_document`: the parts' HTML in order, a rule after each part that asks for a visual separator."""
    out: list[str] = []
    for p in job.parts:
        content = p.get("content")
        if isinstance(content, str):
            out.append(content)
            if p.get("use_section_visual_separator") is True and job.look == "doc":
                out.append("<hr/>")
    return "".join(out)


def doc_header(job: Job) -> str:
    """DOC's `document_html_header`: its header template (the CSS, the title page block) for the layout, rendered the way DOC renders it."""
    name = "html/header_compact_enclosing.html" if job.layout in _COMPACT_LAYOUTS else "html/header_enclosing.html"
    return _TEMPLATES.get_template(name).render(timestring=datetime.now().ctime(), title1=job.title1, title2=job.title2, title3="")


def html_document(job: Job) -> str:
    """The document as one HTML text. `look` doc: DOC's header and footer around the parts (what DOC writes); otherwise the proposed look's skeleton and CSS. Parts are the core's own HTML (already escaped by the core)."""
    body = part_content(job)
    if job.look == "doc":
        return f"{doc_header(job)}{body}</body></html>"
    title = html.escape(f"{job.title1} {job.title2}".strip())
    return f'<!doctype html>\n<html><head><meta charset="utf-8"><title>{title}</title><style>{style_css(LOOK)}</style></head><body>{body}</body></html>\n'


class _WeasyHTML(Protocol):
    def write_pdf(self) -> bytes: ...


class _WeasyModule(Protocol):
    def HTML(self, *, string: str) -> _WeasyHTML: ...  # noqa: N802


def weasyprint_bytes(document: str) -> bytes:
    try:
        weasy: _WeasyModule = importlib.import_module("weasyprint")
    except ImportError as e:
        raise JobError("unavailable", "PDF output needs WeasyPrint, which is not installed in this Python") from e
    return weasy.HTML(string=document).write_pdf()


def prince_binary() -> str | None:
    return shutil.which(os.environ.get("PRINCE_BIN", "prince"))


def prince_bytes(document: str, binary: str, workdir: Path) -> bytes:
    """DOC's Prince call (`prince <html> -o <pdf>`) on a temporary file next to the output; its own time limit so a stuck Prince cannot outlive the stage."""
    with tempfile.TemporaryDirectory(dir=workdir, prefix="prince-") as tmp:
        source = Path(tmp) / "in.html"
        target = Path(tmp) / "out.pdf"
        source.write_text(document, encoding="utf-8")
        try:
            done = subprocess.run([binary, str(source), "-o", str(target)], capture_output=True, text=True, timeout=float(os.environ.get("PRINCE_TIMEOUT_S", "900")), check=False)
        except subprocess.TimeoutExpired as e:
            raise JobError("render", f"prince did not finish within {e.timeout:.0f} s") from e
        if done.returncode != 0 or not target.exists():
            raise JobError("render", f"prince exited {done.returncode}: {done.stderr.strip()[:500]}")
        return target.read_bytes()


def pdf_bytes(document: str, job: Job) -> tuple[bytes, str, str]:
    """The PDF engine: (bytes, engine used, note). Prince when asked for (the request's `use_prince`, or the service's `prince` setting) and installed, else WeasyPrint with the reason as the note, so the shell can return it as a finding. A PDF is produced either way."""
    want_prince = job.pdf_engine == "prince" or (job.pdf_engine == "auto" and job.use_prince)
    note = ""
    if want_prince:
        binary = prince_binary()
        if binary is None:
            note = "Prince was asked for but is not installed in this image; the PDF was made with WeasyPrint"
        else:
            try:
                return prince_bytes(document, binary, job.output.parent), "prince", ""
            except JobError as e:
                note = f"Prince failed ({e}); the PDF was made with WeasyPrint"
                log("warn", "prince failed, falling back to weasyprint", req_id=job.request_id, job=job.job_id, error=str(e))
    return weasyprint_bytes(document), "weasyprint", note


def render(job: Job) -> dict[str, Scalar]:
    started = time.perf_counter()
    log("info", "render start", req_id=job.request_id, job=job.job_id, key=job.request_key, format=job.fmt, parts=len(job.parts))
    fault()
    engine, note = "", ""
    try:
        if job.fmt == "docx":
            data, warnings = gen_docx.build(job.parts, job.book, job.title1, job.title2)
            for w in sorted(set(warnings)):
                log("warn", "generator warning", req_id=job.request_id, job=job.job_id, warning=w)
        elif job.fmt == "html":
            data = html_document(job).encode("utf-8")
        else:
            data, engine, note = pdf_bytes(html_document(job), job)
    except JobError:
        raise
    except Exception as e:  # the generators are libraries: whatever they raise is a render failure, reported typed
        raise JobError("render", f"{type(e).__name__}: {e}") from e
    try:
        tmp = job.output.with_name(job.output.name + ".part")
        with open(tmp, "wb") as f:
            f.write(data)
            f.flush()
            os.fsync(f.fileno())
        os.replace(tmp, job.output)
    except OSError as e:
        raise JobError("render", f"cannot write output: {e}") from e
    ms = round((time.perf_counter() - started) * 1000)
    log("info", "render done", req_id=job.request_id, job=job.job_id, key=job.request_key, bytes=len(data), ms=ms)
    return {
        "ok": True,
        "bytes": len(data),
        "ms": ms,
        "max_rss_kb": resource.getrusage(resource.RUSAGE_SELF).ru_maxrss,
        "rss_kb": rss_kb(),
        "python": platform.python_version(),
        "engine": engine,
        "note": note,
    }
