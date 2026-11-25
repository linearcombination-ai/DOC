"""Celery's fixed cost per task on this host: N round trips of the built-in no-op `celery.ping` task through DOC's
own Celery app (Redis broker and result backend), as the API does it (apply_async, then poll AsyncResult.state)."""
import json, statistics, sys, time
from doc.domain.worker import app
ts = []
for _ in range(int(sys.argv[1])):
    t = time.perf_counter()
    r = app.send_task("p5.noop")
    while r.state not in ("SUCCESS", "FAILURE"):
        time.sleep(0.002)
    ts.append(time.perf_counter() - t)
print(json.dumps({"n": len(ts), "p50_s": statistics.median(ts), "min_s": min(ts), "max_s": max(ts), "first_s": ts[0]}))
