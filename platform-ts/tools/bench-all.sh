#!/usr/bin/env bash
# bench-all.sh doc|p5 <out prefix>: three cold runs (fresh processes, empty asset and output directories), then on the last
# warm process set 10 runs with the output removed each time (assets cached: "warm") and 10 runs with the output kept ("cache hit").
set -euo pipefail
TOOLS="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SYS="$1"; OUT="$2"
source "$TOOLS/doc-celery/env.sh"
BODY="$LAB/p5/doc-run/req.json"
if [ "$SYS" = doc ]; then START="$TOOLS/doc-celery/start-doc.sh"; STOP="$TOOLS/doc-celery/stop-doc.sh"; BASE="http://127.0.0.1:18202"; OUTDIR="$RUN/out"
else START="$TOOLS/run-p5.sh start"; STOP="$TOOLS/run-p5.sh stop"; BASE="http://127.0.0.1:18301"; OUTDIR="$LAB/p5/p5-run/out"; fi
rss() { for p in "$@"; do awk '/VmRSS/{s+=$2}END{print s+0}' /proc/$p/status 2>/dev/null; done | paste -sd+ | bc; }
procs() {
  if [ "$SYS" = doc ]; then echo "uvicorn=$(cat "$RUN/uvicorn.pid"),celery=$(cat "$RUN/celery.pid"),sidecar=$(pgrep -f USFMParserApi.dll | head -1),redis=$(cat "$RUN/redis.pid")"
  else P="$LAB/p5/p5-run"; echo "api=$(cat "$P/api.pid"),worker=$(cat "$P/worker.pid"),redis=$(cat "$P/redis.pid")"; fi
}
rm -f "$OUT".*.jsonl "$OUT".*.mem.json
echo "host load at start: $(cut -d' ' -f1-3 /proc/loadavg)" | tee "$OUT.host.txt"
for i in 1 2 3; do
  bash -c "$STOP" > /dev/null 2>&1 || true; sleep 1
  bash -c "$START cold" > /dev/null
  sleep 3
  echo "cold $i: load $(cut -d' ' -f1 /proc/loadavg), idle RSS kB (sum of trees): $(procs | tr ',' '\n' | while IFS== read n p; do echo -n "$n=$(for q in $p $(pgrep -P $p); do awk '/VmRSS/{print $2}' /proc/$q/status; done | paste -sd+ | bc) "; done)" | tee -a "$OUT.host.txt"
  BENCH_RESULT="$OUT.cold.jsonl" node "$TOOLS/bench-jobs.ts" "$BASE" "$SYS-cold" "$BODY" "$OUTDIR" 1 --procs="$(procs)" | tail -2
done
BENCH_RESULT="$OUT.warm.jsonl" node "$TOOLS/bench-jobs.ts" "$BASE" "$SYS-warm" "$BODY" "$OUTDIR" 10 --procs="$(procs)" | tail -3
BENCH_RESULT="$OUT.hit.jsonl" node "$TOOLS/bench-jobs.ts" "$BASE" "$SYS-hit" "$BODY" "$OUTDIR" 1 --procs="$(procs)" > /dev/null
BENCH_RESULT="$OUT.hit.jsonl" node "$TOOLS/bench-jobs.ts" "$BASE" "$SYS-hit" "$BODY" "$OUTDIR" 10 --keep-output --procs="$(procs)" | tail -3
echo "host load at end: $(cut -d' ' -f1-3 /proc/loadavg)" | tee -a "$OUT.host.txt"
bash -c "$STOP" || true
