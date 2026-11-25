#!/usr/bin/env bash
# One benchmark pass of one server on a freshly started process with an empty asset directory.
#   tools/bench-run.sh fastapi|shell <rep>
# Waits (sleeping, not spinning) for the 1-minute load average to drop below 3 for up to 20 minutes;
# if it does not, runs anyway and says so. Load and process count are written next to the numbers.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$HERE/oracle/env.sh"
P4="$REPO/p4-ts"; WHICH="${1:?fastapi|shell}"; REP="${2:?rep}"
export PATH="$LAB/node/bin:$PATH"
OUT="$P4/results/bench-$WHICH-$REP"
waited=0; load=$(cut -d' ' -f1 /proc/loadavg)
while [ "$(echo "$load >= 3" | bc)" = 1 ] && [ $waited -lt 1200 ]; do sleep 20; waited=$((waited+20)); load=$(cut -d' ' -f1 /proc/loadavg); done
{ echo "load1_at_start: $load"; echo "waited_s: $waited"; echo "runnable_processes: $(awk '{print $4}' /proc/loadavg)"; echo "labelled: $([ "$(echo "$load < 3" | bc)" = 1 ] && echo quiet || echo UNDER-LOAD)"; } > "$OUT.env.txt"
if [ "$WHICH" = fastapi ]; then
  rm -rf "$RESOURCE_ASSETS_DIR"; mkdir -p "$RESOURCE_ASSETS_DIR" "$WORKING_DIR" "$DOCUMENT_OUTPUT_DIR"; PORT_=18202
  cd "$REPO"; "$LAB/venv/bin/python" -m uvicorn doc.entrypoints.app:app --host 127.0.0.1 --port $PORT_ --log-level warning > "$LAB/oracle-work/uvicorn-bench.log" 2>&1 &
else
  PORT_=18301; export PORT=$PORT_ DOC_MODE=as_built LOG_LEVEL=warn RESOURCE_ASSETS_DIR="$LAB/shell-work/bench/assets"
  rm -rf "$LAB/shell-work/bench"; mkdir -p "$RESOURCE_ASSETS_DIR"; cd "$P4"; node src/main.ts > "$LAB/shell-work/bench.log" 2>&1 &
fi
PID=$!; trap 'kill $PID 2>/dev/null || true' EXIT
for _ in $(seq 1 300); do curl -sf "http://127.0.0.1:$PORT_/health/status" >/dev/null && break; sleep 0.2; done
echo "rss_after_start: $(grep -E 'VmRSS' /proc/$PID/status | tr -s ' \t' ' ')" >> "$OUT.env.txt"
cd "$P4"; node tools/bench.ts "http://127.0.0.1:$PORT_" "$WHICH-$REP" "$OUT.json" "$PID"
echo "load1_at_end: $(cut -d' ' -f1 /proc/loadavg)" >> "$OUT.env.txt"
