"""PDF worker: one HTML file in, one PDF out. The engine is swappable behind this one script.

    python3 -I pdf_worker.py --job JOB.json

JOB.json (version 1): ``job_id``, ``request_id``, ``input_html``, ``output_path``, ``engine``. Engines:

* ``weasyprint`` (default): the WeasyPrint library, imported here (the interpreter that runs this script must have it);
* ``prince``: the ``prince`` command (``prince INPUT -o OUTPUT``); DOC's licence notes for Prince are the captain's.

One answer line on stdout (``ok``, ``bytes``, ``worker_ms``, ``maxrss_kb``, ``engine``, ``pid``); exit 0 done, 2 the job is not
acceptable (permanent), 3 an internal error (transient). JSON log lines on stderr carry the request id, as in docx_worker.py.
Fault injection for tests (never set in production): PDF_WORKER_FAULT = hang | crash | exit3, applied to the first
PDF_WORKER_FAULT_TIMES runs (counted in the file PDF_WORKER_FAULT_STATE).
"""

from __future__ import annotations

import argparse
import json
import os
import resource
import shutil
import signal
import subprocess
import sys
import threading
import time
from pathlib import Path

SUPPORTED_VERSIONS = {1}


def _log(event: str, request_id: str, job_id: str, **fields: str | int | float) -> None:
    rec: dict[str, str | int | float] = {"ts": round(time.time(), 3), "level": "info", "event": event, "request_id": request_id, "job_id": job_id, "pid": os.getpid()}
    rec.update(fields)
    sys.stderr.write(json.dumps(rec) + "\n")
    sys.stderr.flush()


def _fault_due() -> str | None:
    fault = os.environ.get("PDF_WORKER_FAULT", "")
    if not fault:
        return None
    times = int(os.environ.get("PDF_WORKER_FAULT_TIMES", "1000000"))
    state = os.environ.get("PDF_WORKER_FAULT_STATE", "")
    seen = 0
    if state:
        p = Path(state)
        seen = int(p.read_text()) if p.exists() else 0
        p.write_text(str(seen + 1))
    return fault if seen < times else None


def _exit_when_orphaned(parent: int) -> None:
    while True:
        time.sleep(0.25)
        if os.getppid() != parent:
            os._exit(4)


def _weasyprint(src: Path, dst: Path) -> None:
    from weasyprint import HTML  # type: ignore[import-untyped]  # noqa: PLC0415 (imported late: the module is heavy)

    HTML(filename=str(src)).write_pdf(str(dst))


def _prince(src: Path, dst: Path) -> None:
    exe = shutil.which("prince")
    if exe is None:
        raise RuntimeError("the prince command is not installed")
    subprocess.run([exe, str(src), "-o", str(dst)], check=True, capture_output=True, timeout=3600)


def main(argv: list[str]) -> int:
    t0 = time.perf_counter()
    threading.Thread(target=_exit_when_orphaned, args=(os.getppid(),), daemon=True).start()
    ap = argparse.ArgumentParser()
    ap.add_argument("--job", required=True)
    args = ap.parse_args(argv)
    try:
        raw: object = json.loads(Path(args.job).read_text(encoding="utf-8"))
        if not isinstance(raw, dict) or raw.get("version") not in SUPPORTED_VERSIONS:
            print("unsupported or malformed PdfJob", file=sys.stderr)
            return 2
        job = {str(k): v for k, v in raw.items()}
        src, dst = Path(str(job["input_html"])), Path(str(job["output_path"]))
        engine, rid, jid = str(job.get("engine", "weasyprint")), str(job.get("request_id", "")), str(job.get("job_id", ""))
    except (OSError, ValueError, KeyError) as e:
        print(f"bad PdfJob: {e}", file=sys.stderr)
        return 2
    if engine not in ("weasyprint", "prince") or not src.is_file():
        print(f"cannot run: engine={engine!r} input exists={src.is_file()}", file=sys.stderr)
        return 2
    _log("pdf_start", rid, jid, engine=engine)
    fault = _fault_due()
    if fault == "hang":
        time.sleep(3600)
    elif fault == "crash":
        os.kill(os.getpid(), signal.SIGSEGV)
    elif fault == "exit3":
        print("injected internal error", file=sys.stderr)
        return 3
    dst.parent.mkdir(parents=True, exist_ok=True)
    try:
        (_prince if engine == "prince" else _weasyprint)(src, dst)
    except Exception as e:  # noqa: BLE001 - any engine failure is transient from the supervisor's view
        print(f"{engine} failed: {type(e).__name__}: {e}", file=sys.stderr)
        return 3
    size = dst.stat().st_size if dst.is_file() else 0
    ms = round((time.perf_counter() - t0) * 1000, 2)
    _log("pdf_done", rid, jid, bytes=size, worker_ms=ms)
    print(json.dumps({"ok": True, "bytes": size, "worker_ms": ms, "maxrss_kb": resource.getrusage(resource.RUSAGE_SELF).ru_maxrss, "engine": engine, "pid": os.getpid()}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
