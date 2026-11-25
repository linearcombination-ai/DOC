#!/usr/bin/env bash
# run-p5.sh start [cold] | stop: this shell (API + worker + Redis + mock data API) beside nothing else, no docker.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$HERE/doc-celery/env.sh"            # same data API, same git mirror rewrite, same paths idiom
source "$LAB/p5/env.sh"
P5RUN="$LAB/p5/p5-run"; mkdir -p "$P5RUN/logs" "$P5RUN/redis"
export REDIS_PORT=26380 PORT=18301 DOC_MODE=fixed LOG_LEVEL="${LOG_LEVEL:-info}"
export REDIS_URL="redis://127.0.0.1:$REDIS_PORT/0" QUEUE_PREFIX=docp5
export RESOURCE_ASSETS_DIR="$P5RUN/assets" WORKING_DIR="$P5RUN/working" DOCUMENT_OUTPUT_DIR="$P5RUN/out"
export PYTHON_BIN="$LAB/venv/bin/python" DOCX_WORKER_SCRIPT="$REPO/p5-ts/python/docx_worker.py"
export WORKER_CONCURRENCY="${WORKER_CONCURRENCY:-2}"
case "$1" in
 start)
  [ "${2:-}" = cold ] && rm -rf "$RESOURCE_ASSETS_DIR" "$WORKING_DIR" "$DOCUMENT_OUTPUT_DIR"
  mkdir -p "$RESOURCE_ASSETS_DIR" "$WORKING_DIR" "$DOCUMENT_OUTPUT_DIR"
  cd "$REPO/p5-ts"
  redis-server --port "$REDIS_PORT" --bind 127.0.0.1 --dir "$P5RUN/redis" --save "" --appendonly no --loglevel warning > "$P5RUN/logs/redis.log" 2>&1 & echo $! > "$P5RUN/redis.pid"
  node tools/mock-data-api.ts "$LAB/p5/snap" 18201 > "$P5RUN/logs/mock.log" 2>&1 & echo $! > "$P5RUN/mock.pid"
  sleep 0.5
  node src/worker-main.ts > "$P5RUN/logs/worker.log" 2>&1 & echo $! > "$P5RUN/worker.pid"
  node src/main.ts > "$P5RUN/logs/api.log" 2>&1 & echo $! > "$P5RUN/api.pid"
  for _ in $(seq 1 100); do curl -sf "http://127.0.0.1:$PORT/health/status" | grep -q '"workers":[1-9]' && break; sleep 0.2; done
  echo "started: api $(cat "$P5RUN/api.pid") worker $(cat "$P5RUN/worker.pid") redis $(cat "$P5RUN/redis.pid")";;
 stop)
  for n in api worker mock redis; do [ -f "$P5RUN/$n.pid" ] && kill "$(cat "$P5RUN/$n.pid")" 2>/dev/null; rm -f "$P5RUN/$n.pid"; done
  sleep 1; pgrep -fa "src/worker-main.ts|src/main.ts|redis-server.*26380|mock-data-api|docx_worker" | grep -v pgrep || echo "all P5 processes stopped";;
esac
