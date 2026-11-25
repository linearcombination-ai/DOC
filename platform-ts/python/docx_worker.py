"""One job per process: `python docx_worker.py JOB.json` (the per-job-process mode of the docx seam).

The job file is the JSON the shell wrote (`src/jobs/docx.ts` is the other half of the contract; see `docx_core.load_job`).
The result is exactly one JSON line on stdout and an exit code:
    0  {"ok": true,  "bytes": N, "ms": N, "max_rss_kb": N, "rss_kb": N, "python": "3.x.y"}
    2  {"ok": false, "kind": "input",  "message": ...}   the job file is not a valid DocxJob (permanent)
    3  {"ok": false, "kind": "render", "message": ...}   the generator failed (the shell decides whether to retry)
    5  {"ok": false, "kind": "unavailable", ...}          the requested format needs something this Python does not have (PDF without WeasyPrint): permanent
    4  (no output)                                         the parent process vanished (orphan watchdog)
Anything else (a signal, a hang, no JSON) is a crash and is the shell's to detect and supervise.
Log lines (JSON) go to stderr. Test-only fault injection: DOCX_WORKER_FAULT (see docx_core.fault).
"""

from __future__ import annotations

import json
import sys

import docx_core
from docx_core import JobError


def main(argv: list[str]) -> int:
    if len(argv) != 1:
        print(json.dumps({"ok": False, "kind": "input", "message": "usage: docx_worker.py JOB.json"}))
        return 2
    docx_core.watch_parent()
    try:
        print(json.dumps(docx_core.render(docx_core.load_job(argv[0]))))
        return 0
    except JobError as e:
        print(json.dumps({"ok": False, "kind": e.kind, "message": str(e)}))
        return {"input": 2, "unavailable": 5}.get(e.kind, 3)


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
