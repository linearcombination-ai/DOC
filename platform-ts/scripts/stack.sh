#!/usr/bin/env bash
# Start, stop and inspect a complete local stack without docker: Redis (or Valkey), the local data stack (catalog mock and git
# remotes, or the real network with REAL_NETWORK=1), one API process and N worker processes. Everything lives under ./run.
#
#   bash scripts/stack.sh start        # needs: node >= 24, core/pkg and core/pkg-api built (bash scripts/build-wasm.sh), npm ci, a Redis or Valkey binary
#   bash scripts/stack.sh status
#   bash scripts/stack.sh logs [api|worker1|redis|data]
#   bash scripts/stack.sh stop         # SIGTERM: the workers drain their running jobs first
#   bash scripts/stack.sh kill-worker  # SIGKILL worker 1 (to watch another worker recover its job)
#   bash scripts/stack.sh worker       # start one more worker (worker2, worker3, ...)
# Environment: REDIS_SERVER_BIN (default redis-server; point it at valkey-server to run on Valkey), API_PORT (18301),
# WORKERS (1), DATA_STACK=external with STACK_ENV=<file> (use a data stack started elsewhere), PYTHON_BIN (python3; WeasyPrint is needed in it for PDF output), DOCX_MODE (persistent | process), SEND_EMAIL/SMTP_* for email.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$HERE"
RUN="$HERE/run"; mkdir -p "$RUN"
API_PORT="${API_PORT:-18301}"; REDIS_PORT="${REDIS_PORT:-16379}"; MOCK_PORT="${MOCK_PORT:-18390}"
REDIS_SERVER_BIN="${REDIS_SERVER_BIN:-redis-server}"

pidof_() { [ -f "$RUN/$1.pid" ] && kill -0 "$(cat "$RUN/$1.pid")" 2>/dev/null && cat "$RUN/$1.pid" || true; }
spawn_() { # name command...
  local name="$1"; shift
  nohup setsid "$@" > "$RUN/$name.log" 2>&1 < /dev/null &
  echo $! > "$RUN/$name.pid"
}
common_env() {
  set -a
  . "${STACK_ENV:-$RUN/stack.env}"
  set +a
  export REDIS_URL="redis://127.0.0.1:$REDIS_PORT/0" PORT="$API_PORT" DOC_MODE=fixed LOG_LEVEL="${LOG_LEVEL:-info}"
  export DOCUMENT_OUTPUT_DIR="$RUN/output" WORKING_DIR="$RUN/work" RESOURCE_ASSETS_DIR="$RUN/assets"
  export DOCX_MODE="${DOCX_MODE:-persistent}" PYTHON_BIN="${PYTHON_BIN:-python3}" QUEUE_PREFIX="${QUEUE_PREFIX:-doclocal}"
  export DOCX_WORKER_SCRIPT="$HERE/python/docx_worker.py" DOCX_SERVER_SCRIPT="$HERE/python/docx_server.py"
}
start_worker() {
  local n="$1"
  ( common_env; spawn_ "worker$n" node "$HERE/src/worker-main.ts" )
  echo "worker$n pid $(cat "$RUN/worker$n.pid")"
}
case "${1:-}" in
  start)
    mkdir -p "$RUN/output" "$RUN/work" "$RUN/assets"
    if [ -z "$(pidof_ redis)" ]; then
      spawn_ redis "$REDIS_SERVER_BIN" --port "$REDIS_PORT" --bind 127.0.0.1 --dir "$RUN" --appendonly yes --appendfsync everysec --save "" --maxmemory-policy noeviction
    fi
    if [ "${DATA_STACK:-}" = "external" ]; then
      :  # the catalog and git remotes are provided elsewhere (STACK_ENV points at their environment file); the benchmark driver does this
    elif [ "${REAL_NETWORK:-0}" = "1" ]; then
      printf 'DATA_API_URL=https://api.bibleineverylanguage.org/v1/graphql\n' > "$RUN/stack.env"
    elif [ -z "$(pidof_ data)" ]; then
      spawn_ data node "$HERE/tools/local-stack.ts" "$RUN" "$MOCK_PORT"
      for _ in $(seq 1 100); do grep -q "local stack ready" "$RUN/data.log" 2>/dev/null && break; sleep 0.1; done
    fi
    for _ in $(seq 1 100); do "${REDIS_SERVER_BIN%server}cli" -p "$REDIS_PORT" ping 2>/dev/null | grep -q PONG && break; sleep 0.1; done
    ( common_env; spawn_ api node "$HERE/src/main.ts" )
    for i in $(seq 1 "${WORKERS:-1}"); do start_worker "$i"; done
    for _ in $(seq 1 100); do curl -sf "http://127.0.0.1:$API_PORT/health/status" > /dev/null && break; sleep 0.2; done
    echo "API http://127.0.0.1:$API_PORT (docs: /openapi.json, metrics: /metrics, health: /health/status); logs in $RUN/*.log"
    ;;
  worker)
    n=1; while [ -n "$(pidof_ worker$n)" ]; do n=$((n+1)); done
    start_worker "$n"
    ;;
  status)
    for p in redis data api worker1 worker2 worker3; do [ -f "$RUN/$p.pid" ] && echo "$p: $( [ -n "$(pidof_ $p)" ] && echo "running pid $(cat "$RUN/$p.pid")" || echo stopped)"; done
    curl -s "http://127.0.0.1:$API_PORT/health/status" || echo "(API not answering)"; echo
    ;;
  logs) tail -n "${LINES:-40}" "$RUN/${2:-api}.log" ;;
  kill-worker) kill -9 "$(cat "$RUN/worker1.pid")" && echo "worker1 SIGKILLed" ;;
  stop)
    for p in worker3 worker2 worker1 api data redis; do
      pid="$(pidof_ $p)"; [ -n "$pid" ] && kill -TERM "$pid" && echo "stopping $p ($pid)"
    done
    for _ in $(seq 1 150); do any=0; for p in worker1 worker2 worker3 api data redis; do [ -n "$(pidof_ $p)" ] && any=1; done; [ $any = 0 ] && break; sleep 0.2; done
    for p in worker1 worker2 worker3 api data redis; do [ -n "$(pidof_ $p)" ] && { echo "$p did not stop in 30 s: SIGKILL"; kill -KILL "$(cat "$RUN/$p.pid")"; }; done
    ;;
  *) sed -n 2,14p "$0"; exit 2 ;;
esac
