#!/usr/bin/env bash
# Source me:   . scripts/stack.sh      (from p6rs/, after `. ./env.sh`)
# A local stack without docker: a data API mirror (python), Redis or Valkey (built by build-servers.sh), and one or
# more doc-jobd processes. The repositories are the fixtures, served by a fake `git` (no network); to use the real
# catalog and the real content host instead: `export DATA_API_URL=https://api.bibleineverylanguage.org/v1/graphql REAL_GIT=1`.
# Functions: stack_up [redis|valkey]   daemon_start NAME [VAR=value ...]   daemon_kill NAME   daemon_stop NAME
#            mirror_down | mirror_up   jude_json   post_docx [JSON]   post_html [JSON]   watch_task ID   stack_down
: "${P6:?source env.sh first}"
S="$P6/scratch/stack"; mkdir -p "$S"
BIN="${DOC_JOBD_BIN:-$(ls -t "$P6"/target/fast/doc-jobd "$P6"/target/release/doc-jobd "$P6"/target/debug/doc-jobd 2>/dev/null | head -1)}"
PORT_REDIS="${PORT_REDIS:-6399}"; PORT_MIRROR="${PORT_MIRROR:-8099}"
portof() { case "$1" in a) echo 8081;; b) echo 8082;; c) echo 8083;; *) echo 8090;; esac; }

stack_up() {
  local server="${1:-redis}"
  python3 -I "$P6/scripts/mock_data_api.py" --port "$PORT_MIRROR" > "$S/mirror.log" 2>&1 & echo $! > "$S/mirror.pid"
  mkdir -p "$S/redis"
  "$P6/.p6env/$server/bin/$server-server" --port "$PORT_REDIS" --bind 127.0.0.1 --dir "$S/redis" --save "" --appendonly yes --appendfsync everysec > "$S/redis.log" 2>&1 & echo $! > "$S/redis.pid"
  sleep 1
  daemon_start a
}

daemon_start() {
  local name="$1"; shift
  local port; port=$(portof "$name")
  local env=(
    "DATA_API_URL=${DATA_API_URL:-http://127.0.0.1:$PORT_MIRROR/v1/graphql}" "DOC_SHELL_ADDR=127.0.0.1:$port" "DOC_SHELL_LOG_FORMAT=json"
    "DOC_SHELL_CACHE_DIR=$S/cache" "DOC_JOBD_REDIS_URL=redis://127.0.0.1:$PORT_REDIS/0" "DOC_JOBD_OUTPUT_DIR=$S/out"
    "DOC_JOBD_DOCX_WORKER=$P6/python/docx_worker.py" "DOC_TEMPLATES_DIR=$P6/../backend/templates/html" "DOC_JOBD_WORKER_ID=$name"
    "DOC_JOBD_LEASE_TTL_MS=${LEASE_MS:-4000}" "DOC_JOBD_REAP_EVERY_MS=500" "DOC_JOBD_BEAT_EVERY_MS=1000" "RUST_LOG=${RUST_LOG:-info}" "PATH=$PATH"
  )
  if [ -z "${REAL_GIT:-}" ]; then env+=("DOC_SHELL_GIT_BIN=$P6/scripts/fake-git.sh" "FAKEGIT_CONTENT=$P6/fixtures/content"); fi
  env "${env[@]}" "$@" "$BIN" > "$S/$name.log" 2>&1 & echo $! > "$S/$name.pid"
  for _ in $(seq 1 100); do curl -fsS "http://127.0.0.1:$port/health/status" > /dev/null 2>&1 && { echo "daemon $name up on :$port (pid $(cat "$S/$name.pid"))"; return 0; }; sleep 0.1; done
  echo "daemon $name did not start; see $S/$name.log" >&2; tail -5 "$S/$name.log" >&2; return 1
}
daemon_kill() { kill -KILL "$(cat "$S/$1.pid")" 2>/dev/null && echo "daemon $1 killed (SIGKILL)"; }
daemon_stop() { kill -TERM "$(cat "$S/$1.pid")" 2>/dev/null && echo "daemon $1 told to stop (SIGTERM, drains running jobs)"; }
mirror_down() { kill -USR1 "$(cat "$S/mirror.pid")"; }
mirror_up() { kill -USR1 "$(cat "$S/mirror.pid")"; }

jude_json() { # extra JSON fields merged in, e.g. jude_json '{"layout_for_print": true}'
  python3 -I - "${1:-{\}}" <<'PY'
import json, sys
body = {"assembly_strategy_kind": "lbo", "document_request_source": "ui", "generate_docx": True, "generate_pdf": False,
        "resource_requests": [{"lang_code": "en", "resource_type": "ulb", "book_code": "jud"}, {"lang_code": "en", "resource_type": "tn", "book_code": "jud"}]}
body.update(json.loads(sys.argv[1]))
print(json.dumps(body))
PY
}
post_docx() { curl -sS -i -X POST "http://127.0.0.1:${PORT_API:-8081}/documents_docx" -H 'content-type: application/json' -d "${1:-$(jude_json)}"; }
post_html() { curl -sS -i -X POST "http://127.0.0.1:${PORT_API:-8081}/documents" -H 'content-type: application/json' -d "${1:-$(jude_json '{"generate_docx": false}')}"; }
watch_task() { # prints each new state once
  local last=""; for _ in $(seq 1 600); do
    local s; s=$(curl -sS "http://127.0.0.1:${PORT_API:-8081}/task_status/$1")
    [ "$s" != "$last" ] && { echo "$s"; last="$s"; }
    case "$s" in *'"SUCCESS"'*|*'"FAILURE"'*) return 0;; esac; sleep 0.1; done; }

stack_down() {
  for n in a b c; do [ -f "$S/$n.pid" ] && kill -TERM "$(cat "$S/$n.pid")" 2>/dev/null; done
  sleep 1
  for f in redis mirror a b c; do [ -f "$S/$f.pid" ] && { kill "$(cat "$S/$f.pid")" 2>/dev/null; rm -f "$S/$f.pid"; }; done
  echo "stack stopped"
}
