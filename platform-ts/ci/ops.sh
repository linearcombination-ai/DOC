#!/usr/bin/env bash
# Operations scenarios against the running compose stack (run in the directory of docker-compose.yml, after the stack is healthy):
#   OUT=ci-out SOAK_MINUTES=8 bash ci/ops.sh [scenario ...]        scenarios: limits drain kill valkey api burst rollback soak   (default: all, in that order)
# One line per check: PASS/FAIL name seconds detail. Exit 1 if any FAIL. Evidence (logs, JSON) goes to $OUT/ops-*.
# The scenarios are the P6 fault tests carried over to containers: SIGTERM drain, SIGKILL mid-job, Valkey restart, API kill, burst, soak, rollback.
set -uo pipefail
API="${API:-http://localhost:5005}"; FILES="${FILES:-http://localhost:8089}"
OUT="${OUT:-ci-out}"; mkdir -p "$OUT"
SOAK_MINUTES="${SOAK_MINUTES:-8}"; SOAK_RATE="${SOAK_RATE:-1}"
LG="python3 ci/loadgen.py $API"
fail=0
now() { date +%s; }
pass() { printf 'PASS %-34s %5ss %s\n' "$1" "$2" "${3:-}"; }
bad() { printf 'FAIL %-34s %5ss %s\n' "$1" "$2" "${3:-}"; fail=1; }
cid() { docker compose ps -a -q "$1" | head -1; }
state_text() { curl -fsS --max-time 10 "$API/task_status/$1" 2>/dev/null | python3 -c 'import sys,json; d=json.load(sys.stdin); print(d["state"])' 2>/dev/null || echo UNREACHABLE; }
wait_running() { # service seconds: running (and healthy when it has a check)
  local svc="$1" deadline=$(( $(now) + $2 )) id h s
  while [ "$(now)" -lt "$deadline" ]; do
    id=$(cid "$svc"); s=$(docker inspect -f '{{.State.Status}}' "$id" 2>/dev/null); h=$(docker inspect -f '{{if .State.Health}}{{.State.Health.Status}}{{end}}' "$id" 2>/dev/null)
    [ "$s" = running ] && { [ -z "$h" ] || [ "$h" = healthy ]; } && return 0
    sleep 2
  done
  return 1
}
until_processing() { # task id, timeout: wait until the job has left the queue and is working (any state but the first two)
  local deadline=$(( $(now) + $2 )) st
  while [ "$(now)" -lt "$deadline" ]; do
    st=$(curl -fsS --max-time 10 "$API/task_status/$1" 2>/dev/null | python3 -c 'import sys,json; d=json.load(sys.stdin); print(d["state"])' 2>/dev/null)
    case "$st" in ""|PENDING|"Receiving request"|SUCCESS|FAILURE) ;; *) echo "$st"; return 0;; esac
    sleep 0.3
  done
  return 1
}
ALL=(limits drain kill valkey api burst rollback soak)
LIST=("$@"); [ "${#LIST[@]}" -eq 0 ] && LIST=("${ALL[@]}")
want() { for w in "${LIST[@]}"; do [ "$w" = "$1" ] && return 0; done; return 1; }

# ---------------------------------------------------------------------------------------------------------------------------------------------
if want limits; then
  t=$(now); docker compose ps -a > "$OUT/ops-ps-start.txt"
  # resource limits actually applied (docker inspect), restart policy and stop grace period per container
  {
    echo "service memory_limit_mb nano_cpus restart_policy stop_timeout"
    for s in valkey api worker gateway frontend; do
      id=$(cid "$s"); [ -z "$id" ] && continue
      docker inspect -f "$s {{div .HostConfig.Memory 1048576}} {{.HostConfig.NanoCpus}} {{.HostConfig.RestartPolicy.Name}} {{if .Config.StopTimeout}}{{.Config.StopTimeout}}{{else}}default{{end}}" "$id"
    done
  } | tee "$OUT/ops-limits.txt"
  if awk 'NR>1 && $2==0 {exit 1}' "$OUT/ops-limits.txt"; then pass limits-applied "$(( $(now) - t ))" "every service has a memory limit"; else bad limits-applied "$(( $(now) - t ))" "a service runs without a memory limit"; fi
fi

# ---------------------------------------------------------------------------------------------------------------------------------------------
if want drain; then
  t=$(now)
  id=$($LG submit doc 29) && stage=$(until_processing "$id" 60)
  echo "drain: task $id in state '$stage' when SIGTERM is sent"
  s0=$(now); docker compose stop -t 130 worker >/dev/null 2>&1; stop_s=$(( $(now) - s0 ))
  code=$(docker inspect -f '{{.State.ExitCode}}' "$(cid worker)" 2>/dev/null)
  docker compose logs --no-color --timestamps worker > "$OUT/ops-drain-worker.log" 2>&1
  final=$(state_text "$id")
  # a job in flight when SIGTERM arrives either finishes during the drain or is recovered by the restarted worker: it must end SUCCESS either way
  docker compose start worker >/dev/null 2>&1; wait_running worker 120
  after=$($LG wait "$id" 180 | python3 -c 'import sys,json; print(json.load(sys.stdin)["state"])' 2>/dev/null)
  if [ "$code" = 0 ] && grep -q "draining the running ones" "$OUT/ops-drain-worker.log" && [ "$after" = SUCCESS ]; then pass sigterm-drain "$(( $(now) - t ))" "exit $code after ${stop_s}s; state at exit: $final; final: $after"; else bad sigterm-drain "$(( $(now) - t ))" "exit=$code stop=${stop_s}s state_at_exit=$final final=$after"; fi
fi

# ---------------------------------------------------------------------------------------------------------------------------------------------
if want kill; then
  t=$(now)
  id=$($LG submit docx 28) && stage=$(until_processing "$id" 60)
  echo "kill: task $id in state '$stage' when the worker is SIGKILLed"
  docker kill -s KILL "$(cid worker)" >/dev/null 2>&1; k0=$(now)
  restarted=no; for _ in $(seq 1 30); do sleep 2; [ "$(docker inspect -f '{{.State.Status}}' "$(cid worker)" 2>/dev/null)" = running ] && { restarted=yes; break; }; done
  [ "$restarted" = no ] && docker compose start worker >/dev/null 2>&1
  wait_running worker 120
  r_s=$(( $(now) - k0 ))
  res=$($LG wait "$id" 240); state=$(echo "$res" | python3 -c 'import sys,json; print(json.load(sys.stdin)["state"])' 2>/dev/null)
  docker compose logs --no-color --timestamps worker > "$OUT/ops-kill-worker.log" 2>&1
  if [ "$state" = SUCCESS ]; then pass sigkill-midjob-recovery "$(( $(now) - t ))" "document arrived after SIGKILL; restart policy acted by itself: $restarted (worker healthy again after ${r_s}s)"; else bad sigkill-midjob-recovery "$(( $(now) - t ))" "state=$state restarted_by_policy=$restarted $res"; fi
  [ "$restarted" = yes ] && pass restart-policy "$r_s" "unless-stopped restarted the SIGKILLed worker" || bad restart-policy "$r_s" "the SIGKILLed worker was not restarted by the policy within 60 s (started by hand)"
fi

# ---------------------------------------------------------------------------------------------------------------------------------------------
if want valkey; then
  t=$(now)
  old=$($LG submit doc 1) && $LG wait "$old" 120 >/dev/null
  pending=$($LG submit docx 27)       # in flight across the restart
  docker compose restart valkey >/dev/null 2>&1; wait_running valkey 60
  kept=$(state_text "$old")
  fin=$($LG wait "$pending" 240 | python3 -c 'import sys,json; print(json.load(sys.stdin)["state"])' 2>/dev/null)
  new=$($LG submit doc 12) && ns=$($LG wait "$new" 120 | python3 -c 'import sys,json; print(json.load(sys.stdin)["state"])' 2>/dev/null)
  docker compose exec -T valkey sh -c 'valkey-cli info persistence | grep -E "aof_enabled|aof_last_write_status|loading"' > "$OUT/ops-valkey-persistence.txt" 2>&1
  if [ "$kept" = SUCCESS ] && [ "$fin" = SUCCESS ] && [ "$ns" = SUCCESS ]; then pass valkey-restart "$(( $(now) - t ))" "old record $kept, in-flight job $fin, new job $ns (AOF)"; else bad valkey-restart "$(( $(now) - t ))" "old=$kept in_flight=$fin new=$ns"; fi
fi

# ---------------------------------------------------------------------------------------------------------------------------------------------
if want api; then
  t=$(now)
  docker kill -s KILL "$(cid api)" >/dev/null 2>&1; k0=$(now)
  restarted=no; for _ in $(seq 1 30); do sleep 2; [ "$(docker inspect -f '{{.State.Status}}' "$(cid api)" 2>/dev/null)" = running ] && { restarted=yes; break; }; done
  [ "$restarted" = no ] && docker compose start api >/dev/null 2>&1
  wait_running api 120
  new=$($LG submit doc 14) && ns=$($LG wait "$new" 120 | python3 -c 'import sys,json; print(json.load(sys.stdin)["state"])' 2>/dev/null)
  # SIGTERM on the API: exits 0 promptly
  s0=$(now); docker compose stop -t 30 api >/dev/null 2>&1; stop_s=$(( $(now) - s0 )); code=$(docker inspect -f '{{.State.ExitCode}}' "$(cid api)" 2>/dev/null)
  docker compose start api >/dev/null 2>&1; wait_running api 120
  if [ "$ns" = SUCCESS ] && [ "$code" = 0 ]; then pass api-kill-and-sigterm "$(( $(now) - t ))" "restart policy acted: $restarted (healthy after $(( $(now) - k0 - stop_s ))s incl. test); new job $ns; SIGTERM exit $code in ${stop_s}s"; else bad api-kill-and-sigterm "$(( $(now) - t ))" "new=$ns sigterm_exit=$code restarted=$restarted"; fi
fi

# ---------------------------------------------------------------------------------------------------------------------------------------------
if want burst; then
  t=$(now)
  bash ci/stats-sampler.sh "$OUT/ops-burst-stats.csv" 2 & sampler=$!
  res=$(python3 ci/loadgen.py "$API" burst 24 600 | tail -1); rc=${PIPESTATUS[0]}
  kill "$sampler" 2>/dev/null
  echo "$res" > "$OUT/ops-burst.json"
  if [ "$rc" = 0 ]; then pass burst-24 "$(( $(now) - t ))" "$res"; else bad burst-24 "$(( $(now) - t ))" "$res"; fi
fi

# ---------------------------------------------------------------------------------------------------------------------------------------------
if want rollback; then
  t=$(now)
  old=$($LG submit doc 5) && $LG wait "$old" 120 >/dev/null; key=$(curl -fsS "$API/task_status/$old" | python3 -c 'import sys,json; print(json.load(sys.stdin)["result"])')
  before=$(docker run --rm -v platform-ts_output:/o:ro alpine sh -c 'ls /o | wc -l')
  # the rollback mechanism: another image tag for the same service, same volumes, same Valkey
  for i in api worker; do docker tag "doc-ts/$i:${IMAGE_TAG:-local}" "doc-ts/$i:rollback"; done
  IMAGE_TAG=rollback docker compose up -d --no-build api worker >/dev/null 2>&1
  wait_running api 120; wait_running worker 120
  img=$(docker inspect -f '{{.Config.Image}}' "$(cid worker)")
  code=$(curl -s -o /dev/null -w '%{http_code}' "$FILES/$key.html")
  new=$($LG submit doc 16) && ns=$($LG wait "$new" 120 | python3 -c 'import sys,json; print(json.load(sys.stdin)["state"])' 2>/dev/null)
  after=$(docker run --rm -v platform-ts_output:/o:ro alpine sh -c 'ls /o | wc -l')
  IMAGE_TAG="${IMAGE_TAG:-local}" docker compose up -d --no-build api worker >/dev/null 2>&1; wait_running api 120; wait_running worker 120
  if [ "$code" = 200 ] && [ "$ns" = SUCCESS ] && [ "$after" -ge "$before" ]; then pass rollback-image-tag "$(( $(now) - t ))" "worker ran $img on the same volumes: earlier document still served ($code), new document $ns, files $before -> $after; switched back"; else bad rollback-image-tag "$(( $(now) - t ))" "earlier=$code new=$ns files $before -> $after image=$img"; fi
fi

# ---------------------------------------------------------------------------------------------------------------------------------------------
if want soak; then
  t=$(now)
  restarts_before=$(for s in api worker valkey gateway; do docker inspect -f '{{.RestartCount}}' "$(cid "$s")"; done | tr '\n' ' ')
  bash ci/stats-sampler.sh "$OUT/ops-soak-stats.csv" 10 & sampler=$!
  python3 ci/loadgen.py "$API" soak "$SOAK_MINUTES" "$SOAK_RATE" "$OUT/ops-soak.json" > "$OUT/ops-soak.txt" 2>&1; rc=$?
  kill "$sampler" 2>/dev/null
  restarts_after=$(for s in api worker valkey gateway; do docker inspect -f '{{.RestartCount}}' "$(cid "$s")"; done | tr '\n' ' ')
  docker compose ps -a > "$OUT/ops-ps-end.txt"
  sum=$(tail -1 "$OUT/ops-soak.txt" | cut -c1-400)
  if [ "$rc" = 0 ] && [ "$restarts_before" = "$restarts_after" ]; then pass "soak-${SOAK_MINUTES}min" "$(( $(now) - t ))" "$sum restarts $restarts_before-> $restarts_after"; else bad "soak-${SOAK_MINUTES}min" "$(( $(now) - t ))" "rc=$rc restarts $restarts_before-> $restarts_after $sum"; fi
fi

[ "$fail" = 0 ] && echo "OPS: all scenarios passed" || echo "OPS: FAILED"
exit "$fail"
