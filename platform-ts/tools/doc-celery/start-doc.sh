#!/usr/bin/env bash
# start-doc.sh [cold]: Redis, the .NET sidecar, uvicorn (DOC's FastAPI), a Celery worker (concurrency 2). Stop with stop-doc.sh.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$HERE/env.sh"
mkdir -p "$RUN/logs" "$RUN/redis"
[ "${1:-}" = cold ] && rm -rf "$RESOURCE_ASSETS_DIR" "$WORKING_DIR" "$DOCUMENT_OUTPUT_DIR"
mkdir -p "$RESOURCE_ASSETS_DIR" "$WORKING_DIR" "$DOCUMENT_OUTPUT_DIR"
cd "$REPO"
redis-server --port "$DOC_REDIS_PORT" --bind 127.0.0.1 --dir "$RUN/redis" --save "" --appendonly no --loglevel warning > "$RUN/logs/redis.log" 2>&1 & echo $! > "$RUN/redis.pid"
ASPNETCORE_URLS="http://127.0.0.1:$USFM_SIDECAR_PORT" nohup "$LAB/dotnet10/dotnet" "$LAB/sidecar2/bin/Release/net10.0/USFMParserApi.dll" > "$RUN/logs/sidecar.log" 2>&1 & echo $! > "$RUN/sidecar.pid"
"$LAB/venv/bin/python" -m uvicorn doc.entrypoints.app:app --host 127.0.0.1 --port "$API_LOCAL_PORT" --log-level warning > "$RUN/logs/uvicorn.log" 2>&1 & echo $! > "$RUN/uvicorn.pid"
"$LAB/venv/bin/celery" --app=doc.domain.worker.app worker --hostname=worker@p6 --loglevel=INFO -E --concurrency=2 > "$RUN/logs/celery.log" 2>&1 & echo $! > "$RUN/celery.pid"
for _ in $(seq 1 240); do curl -sf "http://127.0.0.1:$API_LOCAL_PORT/health/status" >/dev/null && grep -q "ready" "$RUN/logs/celery.log" && break; sleep 0.5; done
echo "started: uvicorn $(cat "$RUN/uvicorn.pid") celery $(cat "$RUN/celery.pid") redis $(cat "$RUN/redis.pid") sidecar $(cat "$RUN/sidecar.pid")  API http://127.0.0.1:$API_LOCAL_PORT  output $DOCUMENT_OUTPUT_DIR"
