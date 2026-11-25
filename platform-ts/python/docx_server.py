"""A persistent document worker: JSON lines on stdin, JSON lines on stdout, many jobs per process.

Protocol (one JSON object per line; the shell sends one request at a time per process):
    ->  {"id": "7", "op": "render", "job_file": "/path/to/job.json"}
    <-  {"id": "7", "ok": true, "bytes": N, "ms": N, "rss_kb": N, "max_rss_kb": N, "python": "3.x", "jobs": N}
    <-  {"id": "7", "ok": false, "kind": "input" | "render", "message": "..."}
    ->  {"id": "8", "op": "ping"}            <-  {"id": "8", "ok": true, "pong": true, "rss_kb": N, "jobs": N}
    ->  {"op": "exit"}                       (exits 0)       EOF on stdin also exits 0
On start, once the generator is imported: {"ready": true, "pid": N, "python": "3.x"}.
stdout is the protocol channel and nothing else writes to it (`sys.stdout` is pointed at stderr for the generator's sake);
log lines go to stderr as JSON. Supervision (deadline, memory ceiling, recycling, restart after a crash) is the shell's.
"""

from __future__ import annotations

import json
import os
import platform
import sys
from typing import TextIO

import docx_core
from docx_core import JobError


def reply(channel: TextIO, payload: dict[str, object]) -> None:
    channel.write(json.dumps(payload) + "\n")
    channel.flush()


def main() -> int:
    channel = os.fdopen(os.dup(1), "w", buffering=1)
    sys.stdout = sys.stderr  # a stray print() in a generator must not corrupt the protocol
    docx_core.watch_parent()
    reply(channel, {"ready": True, "pid": os.getpid(), "python": platform.python_version()})
    jobs = 0
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            request = json.loads(line)
        except ValueError:
            docx_core.log("error", "request is not JSON", line=line[:200])
            continue
        if not isinstance(request, dict):
            continue
        rid = str(request.get("id", ""))
        op = request.get("op")
        if op == "exit":
            return 0
        if op == "ping":
            reply(channel, {"id": rid, "ok": True, "pong": True, "rss_kb": docx_core.rss_kb(), "jobs": jobs})
            continue
        if op != "render" or not isinstance(request.get("job_file"), str):
            reply(channel, {"id": rid, "ok": False, "kind": "input", "message": "expected op render with job_file"})
            continue
        try:
            result = docx_core.render(docx_core.load_job(str(request["job_file"])))
            jobs += 1
            reply(channel, {"id": rid, **result, "jobs": jobs})
        except JobError as e:
            reply(channel, {"id": rid, "ok": False, "kind": e.kind, "message": str(e)})
        except Exception as e:  # noqa: BLE001  never let one bad job end the process; report it typed
            docx_core.log("error", "unexpected failure", error=f"{type(e).__name__}: {e}")
            reply(channel, {"id": rid, "ok": False, "kind": "render", "message": f"{type(e).__name__}: {e}"})
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
