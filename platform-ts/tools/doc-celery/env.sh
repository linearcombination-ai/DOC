# DOC's own FastAPI + Celery + Redis + .NET USFM sidecar, run without docker, for the P6 comparison (latency, DOC's e2e tests).
# Source from anywhere. REPO is the DOC worktree root; LAB its _exp volume (venv, dotnet, sidecar build, redis binary, node).
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export REPO="$(cd "$HERE/../../.." && pwd)"
export LAB="$REPO/_exp" RUN="$REPO/_exp/p6/doc-run"
# the real catalog by default; DATA_API_URL=http://127.0.0.1:PORT/v1/graphql points it at a mock instead
export DATA_API_URL="${DATA_API_URL:-https://api.bibleineverylanguage.org/v1/graphql}"
export USER_AGENT_STR="wa-doc" X_REQUESTED_WITH_VALUE="WA-Tool-Doc"
export CHECK_USFM=true USE_LOCALIZED_BOOK_NAME=true CHECK_ALL_BOOKS_FOR_LANGUAGE=true
export LOCK_TIMEOUT_SECONDS=300 DATA_API_CACHE_TTL_SECONDS=180 API_LOCAL_PORT=18202 PORT=18202
export ASSET_CACHING_ENABLED=true ASSET_CACHING_PERIOD=30 USE_GIT_CLI=true DOWNLOAD_ASSETS=false
export EMAIL_SEND_SUBJECT=x TO_EMAIL_ADDRESS=fake@example.com IMAGE_TAG=local SEND_EMAIL=false
export BACKEND_CORS_ORIGINS='["http://localhost:5173"]'
export RESOURCE_ASSETS_DIR="$RUN/assets" WORKING_DIR="$RUN/working" DOCUMENT_OUTPUT_DIR="$RUN/out"
export LOGGING_CONFIG_FILE_PATH="backend/logging_config.yaml"
export PYTHONPATH="$HERE/shim:$REPO/backend" PYTHONDONTWRITEBYTECODE=1
export DOC_REDIS_PORT=26379 USFM_SIDECAR_PORT=5099 USFM_SIDECAR_URL="http://127.0.0.1:5099"
export CELERY_BROKER_URL="redis://127.0.0.1:$DOC_REDIS_PORT/0" CELERY_RESULT_BACKEND="redis://127.0.0.1:$DOC_REDIS_PORT/0"
export DOTNET_ROOT="$LAB/dotnet10" DOTNET_CLI_TELEMETRY_OPTOUT=1
export PATH="$LAB/venv/bin:$LAB/node/bin:$LAB/bin:$LAB/dotnet10:$PATH"
