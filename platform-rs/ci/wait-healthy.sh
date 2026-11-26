#!/usr/bin/env bash
# Wait until every named compose service is running and healthy (running is enough for a service without a health check).
# `docker compose up --wait` is not used: its handling of services that restart while the stack comes up differs between compose versions.
#   bash ci/wait-healthy.sh <timeout seconds> <service>...      (run in the directory of docker-compose.yml)
# Exit 0 when ready, 1 on timeout or when a service is restarting / exited non-zero (the compose state is printed either way).
set -uo pipefail
TIMEOUT="$1"; shift
SERVICES=("$@")
deadline=$(( $(date +%s) + TIMEOUT ))
while true; do
  state=$(docker compose ps -a --format json 2>/dev/null | python3 -c '
import json, sys
want = sys.argv[1:]
rows = {}
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    for item in (json.loads(line) if line.startswith("[") else [json.loads(line)]):
        rows[item["Service"]] = item
bad, waiting = [], []
for name in want:
    r = rows.get(name)
    if r is None:
        waiting.append(name); continue
    st, health = r.get("State"), r.get("Health", "")
    if st in ("restarting", "dead") or (st == "exited" and r.get("ExitCode") != 0):
        bad.append(name + " " + st)
    elif st != "running" or health not in ("", "healthy"):
        waiting.append(name + "(" + st + "/" + (health or "no-healthcheck") + ")")
print("BAD " + ", ".join(bad) if bad else ("WAIT " + ", ".join(waiting) if waiting else "OK"))
' "${SERVICES[@]}")
  case "$state" in
    OK) echo "all services healthy: ${SERVICES[*]}"; docker compose ps -a; exit 0 ;;
    BAD*) echo "$state"; docker compose ps -a; exit 1 ;;
  esac
  if [ "$(date +%s)" -ge "$deadline" ]; then echo "timed out after ${TIMEOUT}s: $state"; docker compose ps -a; exit 1; fi
  sleep 3
done
