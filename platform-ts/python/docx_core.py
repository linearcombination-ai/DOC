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
import signal
import sys
import threading
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Protocol

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
    )


def html_document(job: Job) -> str:
    """The parts in a document skeleton with the profile's CSS. Parts are the core's own HTML (already escaped by the core)."""
    body = "".join(str(p["content"]) for p in job.parts if isinstance(p.get("content"), str))
    title = html.escape(f"{job.title1} {job.title2}".strip())
    return f'<!doctype html>\n<html><head><meta charset="utf-8"><title>{title}</title><style>{style_css(LOOK)}</style></head><body>{body}</body></html>\n'


class _WeasyHTML(Protocol):
    def write_pdf(self) -> bytes: ...


class _WeasyModule(Protocol):
    def HTML(self, *, string: str) -> _WeasyHTML: ...  # noqa: N802


def pdf_bytes(document: str) -> bytes:
    """The PDF engine. WeasyPrint today; Prince would replace this one function (it takes HTML text and returns bytes)."""
    try:
        weasy: _WeasyModule = importlib.import_module("weasyprint")
    except ImportError as e:
        raise JobError("unavailable", "PDF output needs WeasyPrint, which is not installed in this Python") from e
    return weasy.HTML(string=document).write_pdf()


def render(job: Job) -> dict[str, Scalar]:
    started = time.perf_counter()
    log("info", "render start", req_id=job.request_id, job=job.job_id, key=job.request_key, format=job.fmt, parts=len(job.parts))
    fault()
    try:
        if job.fmt == "docx":
            data, warnings = gen_docx.build(job.parts, job.book, job.title1, job.title2)
            for w in sorted(set(warnings)):
                log("warn", "generator warning", req_id=job.request_id, job=job.job_id, warning=w)
        elif job.fmt == "html":
            data = html_document(job).encode("utf-8")
        else:
            data = pdf_bytes(html_document(job))
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
    }
