#!/usr/bin/env bash
# Operations scenarios against the running compose stack (run in the directory of docker-compose.yml, after the stack is healthy):
#   OUT=ci-out SOAK_MINUTES=8 bash ci/ops.sh [scenario ...]        scenarios: limits drain kill pyworker valkey api burst rollback soak   (default: all, in that order)
# One line per check: PASS/FAIL name seconds detail. Exit 1 if any FAIL. Evidence (logs, JSON) goes to $OUT/ops-*.
# The scenarios are the P6 fault tests carried over to containers: SIGTERM drain, SIGKILL mid-job (docker kill and a crash inside the container), Valkey restart, API crash, burst, soak, rollback.
set -uo pipefail
API="${API:-http://localhost:5005}"; FILES="${FILES:-http://localhost:8089}"
OUT="${OUT:-ci-out}"; mkdir -p "$OUT"
SOAK_MINUTES="${SOAK_MINUTES:-8}"; SOAK_RATE="${SOAK_RATE:-1}"
lg() { python3 ci/loadgen.py "$1" "$API" "${@:2}"; }   # lg <command> <args...>: loadgen takes the command first, then the API address
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
crash() { # service process-pattern: SIGKILL the service's main process from inside its container (a crash; `docker kill` counts as a manual stop and is not restarted)
  docker exec "$(cid "$1")" sh -c "for p in /proc/[0-9]*; do pid=\${p#/proc/}; [ \"\$pid\" = \"\$\$\" ] && continue; if tr '\\0' ' ' < \$p/cmdline 2>/dev/null | grep -q '$2'; then kill -9 \$pid; fi; done" >/dev/null 2>&1
}
restart_count() { docker inspect -f '{{.RestartCount}}' "$(cid "$1")" 2>/dev/null; }
until_processing() { # task id, timeout: wait until the job has left the queue and is working (any state but the first two)
  local deadline=$(( $(now) + $2 )) st
  while [ "$(now)" -lt "$deadline" ]; do
    st=$(curl -fsS --max-time 10 "$API/task_status/$1" 2>/dev/null | python3 -c 'import sys,json; d=json.load(sys.stdin); print(d["state"])' 2>/dev/null)
    case "$st" in ""|PENDING|"Receiving request"|SUCCESS|FAILURE) ;; *) echo "$st"; return 0;; esac
    sleep 0.3
  done
  return 1
}
until_state() { # task id, state substring, timeout: wait until the job reports a state that contains the text (the job is then inside that stage)
  local deadline=$(( $(now) + $3 )) st
  while [ "$(now)" -lt "$deadline" ]; do
    st=$(curl -fsS --max-time 10 "$API/task_status/$1" 2>/dev/null | python3 -c 'import sys,json; d=json.load(sys.stdin); print(d["state"])' 2>/dev/null)
    case "$st" in *"$2"*) echo "$st"; return 0;; SUCCESS|FAILURE) return 1;; esac
    sleep 0.2
  done
  return 1
}
ALL=(limits drain kill pyworker valkey api burst rollback soak)
LIST=("$@"); [ "${#LIST[@]}" -eq 0 ] && LIST=("${ALL[@]}")
want() { for w in "${LIST[@]}"; do [ "$w" = "$1" ] && return 0; done; return 1; }

# ---------------------------------------------------------------------------------------------------------------------------------------------
if want limits; then
  t=$(now); docker compose ps -a > "$OUT/ops-ps-start.txt"
  # resource limits actually applied (docker inspect), restart policy and stop grace period per container
  {
    echo "service memory_limit_mb nano_cpus restart_policy stop_timeout"
    for s in valkey pyworker api worker gateway frontend; do
      id=$(cid "$s"); [ -z "$id" ] && continue
      mem=$(docker inspect -f '{{.HostConfig.Memory}}' "$id")
      echo "$s $(( mem / 1048576 )) $(docker inspect -f '{{.HostConfig.NanoCpus}} {{.HostConfig.RestartPolicy.Name}} {{index .Config "StopTimeout"}}' "$id")"
    done
  } | tee "$OUT/ops-limits.txt"
  if awk 'NR>1 && $2==0 {bad=1} END {exit bad}' "$OUT/ops-limits.txt"; then pass limits-applied "$(( $(now) - t ))" "every service has a memory limit"; else bad limits-applied "$(( $(now) - t ))" "a service runs without a memory limit"; fi
fi

# ---------------------------------------------------------------------------------------------------------------------------------------------
if want drain; then
  t=$(now)
  id=$(lg submit doc 29) && stage=$(until_processing "$id" 60)
  echo "drain: task $id in state '$stage' when SIGTERM is sent"
  s0=$(now); docker compose stop -t 130 worker >/dev/null 2>&1; stop_s=$(( $(now) - s0 ))
  code=$(docker inspect -f '{{.State.ExitCode}}' "$(cid worker)" 2>/dev/null)
  docker compose logs --no-color --timestamps worker > "$OUT/ops-drain-worker.log" 2>&1
  final=$(state_text "$id")
  # a job in flight when SIGTERM arrives either finishes during the drain or is recovered by the restarted worker: it must end SUCCESS either way
  docker compose start worker >/dev/null 2>&1; wait_running worker 120
  after=$(lg wait "$id" 180 | python3 -c 'import sys,json; print(json.load(sys.stdin)["state"])' 2>/dev/null)
  if [ "$code" = 0 ] && grep -q "draining" "$OUT/ops-drain-worker.log" && [ "$after" = SUCCESS ]; then pass sigterm-drain "$(( $(now) - t ))" "exit $code after ${stop_s}s; state at exit: $final; final: $after"; else bad sigterm-drain "$(( $(now) - t ))" "exit=$code stop=${stop_s}s state_at_exit=$final final=$after"; fi
fi

# ---------------------------------------------------------------------------------------------------------------------------------------------
if want kill; then
  # (a) `docker kill -s KILL`: the engine treats it as a manual stop, so the restart policy does not act; an operator starts the service again. The job must still be recovered.
  t=$(now)
  id=$(lg submit docx 28) && stage=$(until_processing "$id" 60)
  echo "kill: task $id in state '$stage' when the worker is SIGKILLed with docker kill"
  docker kill -s KILL "$(cid worker)" >/dev/null 2>&1; k0=$(now)
  restarted=no; for _ in $(seq 1 15); do sleep 2; [ "$(docker inspect -f '{{.State.Status}}' "$(cid worker)" 2>/dev/null)" = running ] && { restarted=yes; break; }; done
  [ "$restarted" = no ] && docker compose start worker >/dev/null 2>&1
  wait_running worker 120
  res=$(lg wait "$id" 240); state=$(echo "$res" | python3 -c 'import sys,json; print(json.load(sys.stdin)["state"])' 2>/dev/null)
  docker compose logs --no-color --timestamps worker > "$OUT/ops-kill-worker.log" 2>&1
  if [ "$state" = SUCCESS ]; then pass sigkill-midjob-recovery "$(( $(now) - t ))" "document arrived after docker kill -s KILL; restart policy acted by itself: $restarted (manual kill: started by hand, healthy after $(( $(now) - k0 ))s)"; else bad sigkill-midjob-recovery "$(( $(now) - t ))" "state=$state restarted_by_policy=$restarted $res"; fi
  # (b) a crash of the worker process inside its container (what an OOM kill or a segfault looks like to the engine): the restart policy acts and the job is recovered
  t=$(now)
  id=$(lg submit docx 26) && stage=$(until_processing "$id" 60)
  before=$(restart_count worker)
  echo "crash: task $id in state '$stage' when the worker process is SIGKILLed inside its container"
  crash worker '^/usr/local/bin/doc-jobd'; k0=$(now)
  ok=no; for _ in $(seq 1 30); do sleep 2; [ "$(restart_count worker)" != "$before" ] && [ "$(docker inspect -f '{{.State.Status}}' "$(cid worker)" 2>/dev/null)" = running ] && { ok=yes; break; }; done
  wait_running worker 120; r_s=$(( $(now) - k0 ))
  res=$(lg wait "$id" 240); state=$(echo "$res" | python3 -c 'import sys,json; print(json.load(sys.stdin)["state"])' 2>/dev/null)
  docker compose logs --no-color --timestamps worker > "$OUT/ops-crash-worker.log" 2>&1
  if [ "$ok" = yes ]; then pass restart-policy-worker "$r_s" "unless-stopped restarted the crashed worker (restart count $before -> $(restart_count worker)), healthy again after ${r_s}s"; else bad restart-policy-worker "$r_s" "no restart by the policy within 60 s"; fi
  if [ "$state" = SUCCESS ]; then pass crash-midjob-recovery "$(( $(now) - t ))" "document arrived after the worker process crashed mid-job"; else bad crash-midjob-recovery "$(( $(now) - t ))" "state=$state $res"; fi
fi

# ---------------------------------------------------------------------------------------------------------------------------------------------
if want pyworker; then
  # the Python docx/PDF service dies while it holds a job: the engine restarts it (policy), the worker's call fails as a transient error, the job is retried and ends with a document
  t=$(now)
  id=$(lg submit pdf 24) && stage=$(until_state "$id" "Converting to PDF" 120)
  before=$(restart_count pyworker)
  echo "pyworker: task $id in state '$stage' when the Python service process is SIGKILLed inside its container"
  crash pyworker 'seam_server.py'; k0=$(now)
  ok=no; for _ in $(seq 1 30); do sleep 2; [ "$(restart_count pyworker)" != "$before" ] && [ "$(docker inspect -f '{{.State.Status}}' "$(cid pyworker)" 2>/dev/null)" = running ] && { ok=yes; break; }; done
  wait_running pyworker 120; r_s=$(( $(now) - k0 ))
  res=$(lg wait "$id" 300); state=$(echo "$res" | python3 -c 'import sys,json; print(json.load(sys.stdin)["state"])' 2>/dev/null)
  docker compose logs --no-color --timestamps worker pyworker > "$OUT/ops-pyworker-crash.log" 2>&1
  if [ "$ok" = yes ]; then pass restart-policy-pyworker "$r_s" "unless-stopped restarted the crashed Python service (restart count $before -> $(restart_count pyworker)), healthy again after ${r_s}s"; else bad restart-policy-pyworker "$r_s" "no restart by the policy within 60 s"; fi
  if [ "$state" = SUCCESS ]; then pass pyworker-crash-midjob-recovery "$(( $(now) - t ))" "PDF arrived after the Python service died mid-conversion"; else bad pyworker-crash-midjob-recovery "$(( $(now) - t ))" "state=$state $res"; fi
fi

# ---------------------------------------------------------------------------------------------------------------------------------------------
if want valkey; then
  t=$(now)
  old=$(lg submit doc 1) && lg wait "$old" 120 >/dev/null
  pending=$(lg submit docx 27)       # in flight across the restart
  docker compose restart valkey >/dev/null 2>&1; wait_running valkey 60
  kept=$(state_text "$old")
  fin=$(lg wait "$pending" 240 | python3 -c 'import sys,json; print(json.load(sys.stdin)["state"])' 2>/dev/null)
  new=$(lg submit doc 12) && ns=$(lg wait "$new" 120 | python3 -c 'import sys,json; print(json.load(sys.stdin)["state"])' 2>/dev/null)
  docker compose exec -T valkey sh -c 'valkey-cli info persistence | grep -E "aof_enabled|aof_last_write_status|loading"' > "$OUT/ops-valkey-persistence.txt" 2>&1
  if [ "$kept" = SUCCESS ] && [ "$fin" = SUCCESS ] && [ "$ns" = SUCCESS ]; then pass valkey-restart "$(( $(now) - t ))" "old record $kept, in-flight job $fin, new job $ns (AOF)"; else bad valkey-restart "$(( $(now) - t ))" "old=$kept in_flight=$fin new=$ns"; fi
fi

# ---------------------------------------------------------------------------------------------------------------------------------------------
if want api; then
  t=$(now)
  before=$(restart_count api)
  crash api '^/usr/local/bin/doc-jobd'; k0=$(now)
  ok=no; for _ in $(seq 1 30); do sleep 2; [ "$(restart_count api)" != "$before" ] && [ "$(docker inspect -f '{{.State.Status}}' "$(cid api)" 2>/dev/null)" = running ] && { ok=yes; break; }; done
  wait_running api 120; r_s=$(( $(now) - k0 ))
  new=$(lg submit doc 14) && ns=$(lg wait "$new" 120 | python3 -c 'import sys,json; print(json.load(sys.stdin)["state"])' 2>/dev/null)
  # SIGTERM on the API (docker compose stop): exits 0 promptly
  s0=$(now); docker compose stop -t 30 api >/dev/null 2>&1; stop_s=$(( $(now) - s0 )); code=$(docker inspect -f '{{.State.ExitCode}}' "$(cid api)" 2>/dev/null)
  docker compose start api >/dev/null 2>&1; wait_running api 120
  if [ "$ok" = yes ] && [ "$ns" = SUCCESS ] && [ "$code" = 0 ]; then pass api-crash-and-sigterm "$(( $(now) - t ))" "the policy restarted the crashed API (healthy after ${r_s}s); new job $ns; SIGTERM exit $code in ${stop_s}s"; else bad api-crash-and-sigterm "$(( $(now) - t ))" "restarted=$ok new=$ns sigterm_exit=$code"; fi
fi

# ---------------------------------------------------------------------------------------------------------------------------------------------
if want burst; then
  t=$(now)
  bash ci/stats-sampler.sh "$OUT/ops-burst-stats.csv" 2 & sampler=$!
  res=$(lg burst 24 600); rc=$?; res=$(echo "$res" | tail -1)
  kill "$sampler" 2>/dev/null
  echo "$res" > "$OUT/ops-burst.json"
  if [ "$rc" = 0 ]; then pass burst-24 "$(( $(now) - t ))" "$res"; else bad burst-24 "$(( $(now) - t ))" "$res"; fi
fi

# ---------------------------------------------------------------------------------------------------------------------------------------------
if want rollback; then
  t=$(now)
  old=$(lg submit doc 5) && lg wait "$old" 120 >/dev/null; key=$(curl -fsS "$API/task_status/$old" | python3 -c 'import sys,json; print(json.load(sys.stdin)["result"])')
  before=$(docker run --rm -v doc-rs_output:/o:ro alpine sh -c 'ls /o | wc -l')
  # the rollback mechanism: another image tag for the same service, same volumes, same Valkey
  docker tag "doc-rs/core:${DOC_RS_TAG:-local}" "doc-rs/core:rollback"
  DOC_RS_TAG=rollback docker compose up -d --no-build api worker >/dev/null 2>&1
  wait_running api 120; wait_running worker 120
  img=$(docker inspect -f '{{.Config.Image}}' "$(cid worker)")
  code=$(curl -s -o /dev/null -w '%{http_code}' "$FILES/$key.html")
  new=$(lg submit doc 16) && ns=$(lg wait "$new" 120 | python3 -c 'import sys,json; print(json.load(sys.stdin)["state"])' 2>/dev/null)
  after=$(docker run --rm -v doc-rs_output:/o:ro alpine sh -c 'ls /o | wc -l')
  DOC_RS_TAG="${DOC_RS_TAG:-local}" docker compose up -d --no-build api worker >/dev/null 2>&1; wait_running api 120; wait_running worker 120
  if [ "$code" = 200 ] && [ "$ns" = SUCCESS ] && [ "$after" -ge "$before" ]; then pass rollback-image-tag "$(( $(now) - t ))" "worker ran $img on the same volumes: earlier document still served ($code), new document $ns, files $before -> $after; switched back"; else bad rollback-image-tag "$(( $(now) - t ))" "earlier=$code new=$ns files $before -> $after image=$img"; fi
fi

# ---------------------------------------------------------------------------------------------------------------------------------------------
if want soak; then
  t=$(now)
  restarts_before=$(for s in api worker pyworker valkey gateway; do docker inspect -f '{{.RestartCount}}' "$(cid "$s")"; done | tr '\n' ' ')
  bash ci/stats-sampler.sh "$OUT/ops-soak-stats.csv" 10 & sampler=$!
  lg soak "$SOAK_MINUTES" "$SOAK_RATE" "$OUT/ops-soak.json" > "$OUT/ops-soak.txt" 2>&1; rc=$?
  kill "$sampler" 2>/dev/null
  restarts_after=$(for s in api worker pyworker valkey gateway; do docker inspect -f '{{.RestartCount}}' "$(cid "$s")"; done | tr '\n' ' ')
  docker compose ps -a > "$OUT/ops-ps-end.txt"
  sum=$(tail -1 "$OUT/ops-soak.txt" | cut -c1-400)
  if [ "$rc" = 0 ] && [ "$restarts_before" = "$restarts_after" ]; then pass "soak-${SOAK_MINUTES}min" "$(( $(now) - t ))" "$sum restarts $restarts_before-> $restarts_after"; else bad "soak-${SOAK_MINUTES}min" "$(( $(now) - t ))" "rc=$rc restarts $restarts_before-> $restarts_after $sum"; fi
fi

[ "$fail" = 0 ] && echo "OPS: all scenarios passed" || echo "OPS: FAILED"
exit "$fail"
