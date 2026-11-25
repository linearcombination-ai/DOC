#!/usr/bin/env bash
# Replay a failed (or finished) job: read its request from Redis and post it again.
#   scripts/replay.sh TASK_ID     env: REDIS_CLI (redis-cli), REDIS_PORT (6399), NS (docjobs), API (http://127.0.0.1:8081)
set -euo pipefail
id="${1:?task id}"; ns="${NS:-docjobs}"; cli="${REDIS_CLI:-redis-cli}"; port="${REDIS_PORT:-6399}"; api="${API:-http://127.0.0.1:8081}"
payload=$($cli -p "$port" HGET "$ns:job:$id" payload); kind=$($cli -p "$port" HGET "$ns:job:$id" kind)
[ -n "$payload" ] || { echo "no job record for $id (expired after 24 h, or Redis lost its data)"; exit 1; }
echo "old state: $($cli -p "$port" HGET "$ns:job:$id" st); failure: $($cli -p "$port" HGET "$ns:job:$id" failure)"
path=documents_docx; [ "$kind" = html ] && path=documents
curl -sS -X POST "$api/$path" -H 'content-type: application/json' -d "$payload"; echo
