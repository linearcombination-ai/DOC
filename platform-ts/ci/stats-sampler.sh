#!/usr/bin/env bash
# Sample `docker stats` every N seconds into a CSV until killed:  bash ci/stats-sampler.sh <out.csv> [interval seconds] &
# Columns: epoch,container,mem_usage,mem_percent,cpu_percent,pids  (MemUsage is "used / limit"; the report script parses it).
set -uo pipefail
OUT="$1"; EVERY="${2:-5}"
echo "epoch,container,mem_usage,mem_percent,cpu_percent,pids" > "$OUT"
while true; do
  now=$(date +%s)
  docker stats --no-stream --format '{{.Name}},{{.MemUsage}},{{.MemPerc}},{{.CPUPerc}},{{.PIDs}}' 2>/dev/null | sed "s/^/${now},/" >> "$OUT"
  sleep "$EVERY"
done
