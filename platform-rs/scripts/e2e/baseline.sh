#!/usr/bin/env bash
# DOC's own stack on this host, no docker: Redis, the .NET USFM sidecar, a Celery worker and uvicorn (FastAPI).
#   scripts/e2e/baseline.sh up|down        (from p6rs/, after `. ./env.sh`; the DOC checkout is p6rs/..)
# State under scratch/baseline/. The sidecar is the one built by earlier workers in _exp/sidecar (built from DOC's dotnet/).
set -euo pipefail
: "${P6:?source env.sh first}"
DOC="$P6/.."; W="$P6/scratch/baseline"; mkdir -p "$W"/{redis,out,assets,working,logs}
PY="$DOC/_exp/venv/bin/python"; E="$DOC/_exp"
export CELERY_BROKER_URL=redis://127.0.0.1:6401/0 CELERY_RESULT_BACKEND=redis://127.0.0.1:6401/0
export DOCUMENT_OUTPUT_DIR="$W/out" RESOURCE_ASSETS_DIR="$W/assets" WORKING_DIR="$W/working" API_LOCAL_PORT=5007
export PATH="$DOC/_exp/venv/bin:$PATH"
export PYTHONPATH="$DOC/backend:$P6/scripts/e2e" PYTHONHASHSEED=0 TMPDIR="$P6/.p6env/tmp"
case "${1:-}" in
  up)
    "$P6/.p6env/redis/bin/redis-server" --port 6401 --bind 127.0.0.1 --dir "$W/redis" --save "" --appendonly no > "$W/logs/redis.log" 2>&1 & echo $! > "$W/redis.pid"
    ( export DOTNET_CLI_HOME=$E/dotnet NUGET_PACKAGES=$E/nuget DOTNET_NOLOGO=1 DOTNET_CLI_TELEMETRY_OPTOUT=1 DOTNET_EnableWriteXorExecute=0 DOTNET_ROOT=$E/dotnet10 ASPNETCORE_URLS=http://127.0.0.1:5099
      cd "$E/sidecar/out" && nohup "$E/dotnet10/dotnet" USFMParserApi.dll > "$W/logs/sidecar.log" 2>&1 & echo $! > "$W/sidecar.pid" )
    sleep 6
    ( cd "$DOC" && nohup "$PY" -m celery --app=doc.domain.worker.app worker --hostname=worker@baseline --concurrency=2 --loglevel=INFO > "$W/logs/celery.log" 2>&1 & echo $! > "$W/celery.pid" )
    ( cd "$DOC" && nohup "$PY" -m uvicorn doc.entrypoints.app:app --host 127.0.0.1 --port 5007 --app-dir backend --workers 2 > "$W/logs/uvicorn.log" 2>&1 & echo $! > "$W/uvicorn.pid" )
    for _ in $(seq 1 60); do curl -fsS http://127.0.0.1:5007/health/status > /dev/null 2>&1 && { echo "DOC baseline up on :5007"; exit 0; }; sleep 1; done
    echo "baseline did not come up; see $W/logs" >&2; exit 1;;
  down)
    for f in uvicorn celery sidecar redis; do [ -f "$W/$f.pid" ] && { kill "$(cat "$W/$f.pid")" 2>/dev/null || true; rm -f "$W/$f.pid"; }; done
    pkill -f "celery --app=doc.domain.worker.app" 2>/dev/null || true; echo "baseline stopped";;
  *) echo "usage: baseline.sh up|down"; exit 2;;
esac
