"""P6 docx worker: DocxJob (a JSON object) in, a docx file out, a JSON answer.

Two modes, one job format:

    python3 -I docx_worker.py --job JOB.json    per-job process: read the file, write the docx, print one
                                                answer line, exit (0 done, 2 not acceptable, 3 internal)
    python3 -I docx_worker.py --serve           persistent: one DocxJob per stdin line, one answer line per
                                                job on stdout, until stdin closes; stderr carries JSON log lines

The Rust side supervises both (deadline, kill, restart). In ``--serve`` mode a job that fails answers
``{"ok": false, "exit": 2|3, "message": ...}`` instead of exiting, so one bad job does not cost the
interpreter; a crash or hang is the supervisor's to detect. Every answer carries the worker's current
resident size (``rss_kb``) and the number of jobs it has done, so the supervisor can recycle it.

Logging: one JSON object per line on stderr (``ts``, ``level``, ``event``, ``request_id``, ``job_id``, ...),
the request id being the HTTP request's ``X-Request-Id`` carried in the job.

Fault injection for the test suite (never set in production): DOCX_WORKER_FAULT is one of
hang, hang-grandchild, crash, exit3, reject, bad-output, slow:<seconds>, leak:<KiB>, applied to the first
DOCX_WORKER_FAULT_TIMES invocations (counted in the file DOCX_WORKER_FAULT_STATE), then the worker behaves.
"""

from __future__ import annotations

import argparse
import json
import os
import resource
import signal
import subprocess
import sys
import threading
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent / "docx"))
import gen_docx  # noqa: E402

SUPPORTED_VERSIONS = {1, 2}

Json = dict[str, object]
_LEAKED: list[bytearray] = []
_JOBS_DONE = 0


class JobRejected(Exception):
    """The job cannot be done by retrying it (exit 2)."""


class JobFailed(Exception):
    """An internal failure; a retry may succeed (exit 3)."""


def _log(level: str, event: str, request_id: str, job_id: str, **fields: str | int | float | bool) -> None:
    rec: dict[str, str | int | float | bool] = {
        "ts": round(time.time(), 3),
        "level": level,
        "event": event,
        "request_id": request_id,
        "job_id": job_id,
        "pid": os.getpid(),
    }
    rec.update(fields)
    sys.stderr.write(json.dumps(rec) + "\n")
    sys.stderr.flush()


def _fault_due() -> str | None:
    fault = os.environ.get("DOCX_WORKER_FAULT", "")
    if not fault:
        return None
    times = int(os.environ.get("DOCX_WORKER_FAULT_TIMES", "1000000"))
    state = os.environ.get("DOCX_WORKER_FAULT_STATE", "")
    seen = 0
    if state:
        p = Path(state)
        seen = int(p.read_text()) if p.exists() else 0
        p.write_text(str(seen + 1))
    return fault if seen < times else None


def _str(job: Json, key: str) -> str:
    v = job.get(key)
    if not isinstance(v, str):
        raise JobRejected(f"{key}: expected a string")
    return v


def _opt_str(job: Json, key: str) -> str:
    v = job.get(key, "")
    return v if isinstance(v, str) else ""


def _parts(job: Json) -> list[Json]:
    v = job.get("parts")
    if not isinstance(v, list):
        raise JobRejected("parts: expected a list of objects")
    out: list[Json] = []
    for x in v:
        if not isinstance(x, dict):
            raise JobRejected("parts: expected a list of objects")
        out.append({str(k): val for k, val in x.items()})
    return out


def _rss_kb() -> int:
    """Current resident set size from /proc (``ru_maxrss`` is a peak, useless for a leak check)."""
    try:
        for line in Path("/proc/self/status").read_text().splitlines():
            if line.startswith("VmRSS:"):
                return int(line.split()[1])
    except (OSError, ValueError, IndexError):
        pass
    return resource.getrusage(resource.RUSAGE_SELF).ru_maxrss


def _exit_when_orphaned(parent: int) -> None:
    """If the Rust worker dies (SIGKILL, OOM), nobody will kill this process group: leave on our own."""
    while True:
        time.sleep(0.25)
        if os.getppid() != parent:
            os._exit(4)


def run_job(job: Json, fault: str | None) -> Json:
    """Do one job; raise JobRejected or JobFailed; return the answer object."""
    global _JOBS_DONE
    t0 = time.perf_counter()
    version = job.get("version")
    if version not in SUPPORTED_VERSIONS:
        raise JobRejected(f"unsupported DocxJob version: {version!r}")
    request_id = _opt_str(job, "request_id")
    job_id = _opt_str(job, "job_id")
    out_path = Path(_str(job, "output_path"))
    parts = _parts(job)
    book, t1, t2 = _str(job, "book_name"), _str(job, "title1"), _str(job, "title2")
    _log("info", "docx_start", request_id, job_id, parts=len(parts))

    pidfile = os.environ.get("DOCX_WORKER_PIDFILE", "") if fault is not None else ""
    if pidfile:
        Path(pidfile).write_text(str(os.getpid()))
    if fault == "hang":
        time.sleep(3600)
    elif fault == "hang-grandchild":
        grandchild = subprocess.Popen(["sleep", "3600"])
        if pidfile:
            Path(pidfile).write_text(f"{os.getpid()} {grandchild.pid}")
        time.sleep(3600)
    elif fault == "crash":
        os.kill(os.getpid(), signal.SIGSEGV)
    elif fault == "exit3":
        raise JobFailed("injected internal error")
    elif fault == "reject":
        raise JobRejected("injected rejection")
    elif fault is not None and fault.startswith("slow:"):
        time.sleep(float(fault.split(":", 1)[1]))
    elif fault is not None and fault.startswith("leak:"):
        _LEAKED.append(bytearray(int(fault.split(":", 1)[1]) * 1024))
        _LEAKED[-1][::4096] = b"\x01" * len(_LEAKED[-1][::4096])  # touch the pages so they count as resident

    try:
        data, warnings = gen_docx.build(parts, book, t1, t2)
    except (ValueError, KeyError, IndexError) as e:
        raise JobFailed(f"generator failed: {type(e).__name__}: {e}") from e
    if fault == "bad-output":
        data = data[: len(data) // 2]
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_bytes(data)
    _JOBS_DONE += 1
    worker_ms = round((time.perf_counter() - t0) * 1000, 2)
    _log("info", "docx_done", request_id, job_id, bytes=len(data), worker_ms=worker_ms, warnings=len(set(warnings)))
    return {
        "ok": True,
        "bytes": len(data),
        "worker_ms": worker_ms,
        "maxrss_kb": resource.getrusage(resource.RUSAGE_SELF).ru_maxrss,
        "rss_kb": _rss_kb(),
        "jobs_done": _JOBS_DONE,
        "warnings": sorted(set(warnings)),
        "pid": os.getpid(),
        "job_id": job_id,
    }


def _once(job_file: str) -> int:
    try:
        raw: object = json.loads(Path(job_file).read_text(encoding="utf-8"))
        if not isinstance(raw, dict):
            raise JobRejected("the job is not a JSON object")
        job: Json = {str(k): v for k, v in raw.items()}
        answer = run_job(job, _fault_due())
    except (OSError, json.JSONDecodeError) as e:
        print(f"bad DocxJob: {e}", file=sys.stderr)
        return 2
    except JobRejected as e:
        print(str(e), file=sys.stderr)
        return 2
    except JobFailed as e:
        print(str(e), file=sys.stderr)
        return 3
    print(json.dumps(answer))
    return 0


def _serve() -> int:
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        job_id = ""
        request_id = ""
        try:
            raw: object = json.loads(line)
            if not isinstance(raw, dict):
                raise JobRejected("the job is not a JSON object")
            job: Json = {str(k): v for k, v in raw.items()}
            job_id, request_id = _opt_str(job, "job_id"), _opt_str(job, "request_id")
            answer = run_job(job, _fault_due())
        except json.JSONDecodeError as e:
            answer = {"ok": False, "exit": 2, "message": f"bad DocxJob: {e}", "job_id": job_id}
        except JobRejected as e:
            _log("warn", "docx_rejected", request_id, job_id, message=str(e))
            answer = {"ok": False, "exit": 2, "message": str(e), "job_id": job_id}
        except JobFailed as e:
            _log("warn", "docx_failed", request_id, job_id, message=str(e))
            answer = {"ok": False, "exit": 3, "message": str(e), "job_id": job_id}
        except OSError as e:
            _log("warn", "docx_failed", request_id, job_id, message=str(e))
            answer = {"ok": False, "exit": 3, "message": f"{type(e).__name__}: {e}", "job_id": job_id}
        answer["rss_kb"] = _rss_kb()
        answer["jobs_done"] = _JOBS_DONE
        sys.stdout.write(json.dumps(answer) + "\n")
        sys.stdout.flush()
    return 0


def main(argv: list[str]) -> int:
    threading.Thread(target=_exit_when_orphaned, args=(os.getppid(),), daemon=True).start()
    ap = argparse.ArgumentParser()
    mode = ap.add_mutually_exclusive_group(required=True)
    mode.add_argument("--job", help="per-job mode: the DocxJob file")
    mode.add_argument("--serve", action="store_true", help="persistent mode: DocxJob lines on stdin")
    args = ap.parse_args(argv)
    if args.serve:
        return _serve()
    return _once(str(args.job))


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
