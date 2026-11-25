#!/usr/bin/env bash
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$HERE/env.sh"
for n in celery uvicorn sidecar redis; do [ -f "$RUN/$n.pid" ] && kill "$(cat "$RUN/$n.pid")" 2>/dev/null; rm -f "$RUN/$n.pid"; done
sleep 2
# Celery's prefork children outlive a quick kill of the parent: wait, then check (patterns in brackets so they do not match this script)
pgrep -f "[d]oc.domain.worker.app" >/dev/null && { pkill -f "[d]oc.domain.worker.app"; sleep 2; } || true
pgrep -fa "[d]oc.domain.worker.app|[u]vicorn doc.entrypoints|[U]SFMParserApi.dll" || echo "all DOC processes stopped"
