#!/usr/bin/env bash
# bench-p6.sh ts|doc <result prefix>: the P5 request (English ULB + TN for Jude, docx) against this build or against DOC's own
# FastAPI + Celery stack, on the same local data (catalog mock and git remotes from tools/local-stack.ts), same client, same host.
#   3 cold runs (fresh processes, empty asset and output directories), 10 warm runs (repositories cached, document removed each time),
#   10 cache-hit runs, 5 bursts of 6 different documents at once; peak memory of the process trees; host load recorded.
# Run when the host load is below 3. Needs REDIS_SERVER_BIN (this build) and DOC's venv + sidecar (DOC).
set -euo pipefail
TOOLS="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROTO="$(cd "$TOOLS/.." && pwd)"
SYS="$1"; OUT="$2"
. "$TOOLS/doc-celery/env.sh"; HERE="$PROTO"          # REPO, LAB, RUN (DOC's run directory)
BENCH="$LAB/p6/bench"; mkdir -p "$BENCH/data" "$(dirname "$OUT")"
BODY="$TOOLS/req-jude.json"
if [ -z "$(pgrep -f '[l]ocal-stack.ts' || true)" ]; then
  nohup node "$TOOLS/local-stack.ts" "$BENCH/data" 18390 > "$BENCH/data.log" 2>&1 &
  for _ in $(seq 1 100); do grep -q "local stack ready" "$BENCH/data.log" 2>/dev/null && break; sleep 0.1; done
fi
set -a; . "$BENCH/data/stack.env"; set +a      # DATA_API_URL and GIT_CONFIG_GLOBAL of the local data stack
export STACK_ENV="$BENCH/data/stack.env" DATA_STACK=external
if [ "$SYS" = doc ]; then
  START="$TOOLS/doc-celery/start-doc.sh"; STOP="$TOOLS/doc-celery/stop-doc.sh"; BASE="http://127.0.0.1:18202"; OUTDIR="$RUN/out"
  procs() { echo "uvicorn=$(cat "$RUN/uvicorn.pid"),celery=$(cat "$RUN/celery.pid"),sidecar=$(cat "$RUN/sidecar.pid"),redis=$(cat "$RUN/redis.pid")"; }
else
  START="bash $HERE/scripts/stack.sh start"; STOP="bash $HERE/scripts/stack.sh stop"; BASE="http://127.0.0.1:18301"; OUTDIR="$HERE/run/output"
  procs() { echo "api=$(cat "$HERE/run/api.pid"),worker=$(cat "$HERE/run/worker1.pid"),redis=$(cat "$HERE/run/redis.pid")"; }
fi
tree_kb() { for q in $1 $(pgrep -P "$1" || true); do awk '/VmRSS/{print $2}' "/proc/$q/status" 2>/dev/null; done | paste -sd+ | bc; }
rm -f "$OUT".*.jsonl "$OUT".*.mem.json
echo "host load at start: $(cut -d' ' -f1-3 /proc/loadavg)" | tee "$OUT.host.txt"
for i in 1 2 3; do
  bash -c "$STOP" > /dev/null 2>&1 || true; sleep 1
  [ "$SYS" = ts ] && rm -rf "$HERE/run/assets" "$HERE/run/output" "$HERE/run/work"
  bash -c "$START ${COLD:-cold}" > /dev/null
  sleep 3
  echo "cold $i: load $(cut -d' ' -f1 /proc/loadavg), idle RSS kB: $(procs | tr ',' '\n' | while IFS== read n p; do echo -n "$n=$(tree_kb "$p") "; done)" | tee -a "$OUT.host.txt"
  BENCH_RESULT="$OUT.cold.jsonl" node "$TOOLS/bench-jobs.ts" "$BASE" "$SYS-cold" "$BODY" "$OUTDIR" 1 --procs="$(procs)" | tail -2
done
BENCH_RESULT="$OUT.warm.jsonl" node "$TOOLS/bench-jobs.ts" "$BASE" "$SYS-warm" "$BODY" "$OUTDIR" 10 --procs="$(procs)" | tail -3
BENCH_RESULT="$OUT.hit.jsonl" node "$TOOLS/bench-jobs.ts" "$BASE" "$SYS-hit" "$BODY" "$OUTDIR" 1 > /dev/null
BENCH_RESULT="$OUT.hit.jsonl" node "$TOOLS/bench-jobs.ts" "$BASE" "$SYS-hit" "$BODY" "$OUTDIR" 10 --keep-output --procs="$(procs)" | tail -3
for i in 1 2 3 4 5; do
  rm -rf "$OUTDIR"/* 
  BENCH_RESULT="$OUT.burst.jsonl" node "$TOOLS/bench-burst.ts" "$BASE" "$SYS-burst6" "$BODY" 6 | tail -1
done
echo "host load at end: $(cut -d' ' -f1-3 /proc/loadavg)" | tee -a "$OUT.host.txt"
bash -c "$STOP" || true
pkill -f '[l]ocal-stack.ts' || true
