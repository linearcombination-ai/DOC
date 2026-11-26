#!/usr/bin/env bash
# The container stack as plain processes (no container engine on this host): the same services, the same environment
# variables and the same ports as docker-compose.yml, so everything but the images and the compose wiring can be
# exercised here. Source it after `. ./env.sh`:
#
#   . scripts/stack-p7.sh
#   p7_up            fixtures (mock data API + fake git); or  REAL=1 p7_up  for the real catalog and the real git host
#   p7_status        health of every piece, through the gateway
#   p7_down
#
# State under scratch/stack7/ (out/, assets/, logs/, pids). Ports: valkey 6399, api 8081, worker ops 8082, pyworker 9100,
# gateway 5005 (API) and 8089 (files), mock data API 8099.
: "${PR:?source env.sh first}"
S7="$SCRATCH/stack7"; OUT="$S7/out"; ASSETS="$S7/assets"; LOGS="$S7/logs"
VALKEY_DIR="$(ls -d "$SCRATCH"/servers/valkey-* 2>/dev/null | head -1)"
NGINX_BIN="${NGINX_BIN:-$SCRATCH/servers/nginx/sbin/nginx}"
BIN="${DOC_JOBD_BIN:-$(ls -t "$CARGO_TARGET_DIR"/release/doc-jobd "$CARGO_TARGET_DIR"/debug/doc-jobd 2>/dev/null | head -1)}"
PYBIN="${PYBIN:-$SCRATCH/venv-doc/bin/python}"

p7_env_common() {
  P7_DATA_API="${DATA_API_URL:-http://127.0.0.1:8099/v1/graphql}"
  if [ -n "${REAL:-}" ]; then P7_DATA_API="${DATA_API_URL:-https://api.bibleineverylanguage.org/v1/graphql}"; fi
}

p7_start() { # name, cmd... (env from the caller); records the pid
  local name="$1"; shift
  nohup "$@" > "$LOGS/$name.log" 2>&1 &
  echo $! > "$S7/$name.pid"
}

p7_render_gateway() {
  mkdir -p "$S7/nginx/tmp" "$S7/nginx/logs"
  sed -e "s#server api:8081#server 127.0.0.1:8081#" \
      -e "s#root /app/document_output;#root $OUT;#" \
      -e "s#access_log /dev/stdout doc_json;#access_log $S7/nginx/logs/access.log doc_json;#" \
      -e "s#error_log  /dev/stderr warn;#error_log $S7/nginx/logs/error.log warn;#" \
      "$PR/containers/gateway/default.conf" > "$S7/nginx/gateway.conf"
  cat > "$S7/nginx/nginx.conf" <<EOF
worker_processes 1;
daemon off;
pid $S7/nginx/nginx.pid;
error_log $S7/nginx/logs/error.log warn;
events { worker_connections 256; }
http {
  default_type application/octet-stream;
  client_body_temp_path $S7/nginx/tmp/body;
  proxy_temp_path $S7/nginx/tmp/proxy;
  fastcgi_temp_path $S7/nginx/tmp/fcgi;
  uwsgi_temp_path $S7/nginx/tmp/uwsgi;
  scgi_temp_path $S7/nginx/tmp/scgi;
  include $S7/nginx/gateway.conf;
}
EOF
}

p7_up() {
  p7_env_common
  mkdir -p "$S7" "$OUT" "$ASSETS" "$LOGS" "$S7/valkey"
  chmod 755 "$OUT"
  if [ -z "${REAL:-}" ]; then
    p7_start mirror python3 -I "$PR/scripts/mock_data_api.py" --port 8099
  fi
  p7_start valkey "$VALKEY_DIR/bin/valkey-server" --port 6399 --bind 127.0.0.1 --dir "$S7/valkey" --save "" --appendonly yes --appendfsync everysec --maxmemory-policy noeviction
  PYWORKER_ROOT="$OUT" PYWORKER_PORT=9100 PYWORKER_HOST=127.0.0.1 PYWORKER_CONCURRENCY=2 p7_start pyworker "$PYBIN" -I "$PR/python/seam_server.py"
  local common=(
    "DATA_API_URL=$P7_DATA_API" "DOC_JOBD_REDIS_URL=redis://127.0.0.1:6399/0" "DOC_JOBD_STORE=redis" "DOC_JOBD_OUTPUT_DIR=$OUT"
    "DOC_SHELL_CACHE_DIR=$ASSETS" "DOC_SHELL_LOG_FORMAT=json" "DOC_TEMPLATES_DIR=$WT/backend/templates/html" "RUST_LOG=${RUST_LOG:-info}"
    "BACKEND_CORS_ORIGINS=[\"http://localhost:8001\",\"http://127.0.0.1:8001\"]" "PATH=$PATH"
    "DOC_DOCX_MODE=remote" "DOC_PDF_ENGINE=remote" "DOC_PYWORKER_URL=http://127.0.0.1:9100"
    "DOC_JOBD_LEASE_TTL_MS=${LEASE_MS:-10000}" "DOC_JOBD_REAP_EVERY_MS=1000" "DOC_JOBD_BEAT_EVERY_MS=1000"
  )
  if [ -z "${REAL:-}" ]; then common+=("DOC_SHELL_GIT_BIN=$PR/scripts/fake-git.sh" "FAKEGIT_CONTENT=$PR/fixtures/content"); fi
  env "${common[@]}" "DOC_JOBD_ROLE=api" "DOC_SHELL_ADDR=127.0.0.1:8081" "$@" "$BIN" > "$LOGS/api.log" 2>&1 & echo $! > "$S7/api.pid"
  env "${common[@]}" "DOC_JOBD_ROLE=worker" "DOC_JOBD_OPS_ADDR=127.0.0.1:8082" "DOC_SHELL_ADDR=127.0.0.1:8083" "DOC_JOBD_CONCURRENCY=${WORKER_CONCURRENCY:-2}" \
      "DOC_JOBD_DRAIN_DEADLINE_MS=${DRAIN_MS:-20000}" "$@" "$BIN" > "$LOGS/worker.log" 2>&1 & echo $! > "$S7/worker.pid"
  if [ -x "$NGINX_BIN" ]; then
    p7_render_gateway
    "$NGINX_BIN" -t -c "$S7/nginx/nginx.conf" -p "$S7/nginx/" > "$LOGS/nginx-t.log" 2>&1 || { echo "nginx -t failed:"; cat "$LOGS/nginx-t.log"; return 1; }
    p7_start nginx "$NGINX_BIN" -c "$S7/nginx/nginx.conf" -p "$S7/nginx/"
  else
    echo "no nginx at $NGINX_BIN: the gateway is skipped (API on :8081 only)" >&2
  fi
  local i
  for i in $(seq 1 120); do
    curl -fsS "http://127.0.0.1:${API_PORT:-5005}/health/status" > /dev/null 2>&1 && break
    curl -fsS "http://127.0.0.1:8081/health/status" > /dev/null 2>&1 && [ ! -x "$NGINX_BIN" ] && break
    sleep 0.5
  done
  p7_status
}

p7_status() {
  local api="http://127.0.0.1:8081"; [ -x "$NGINX_BIN" ] && api="http://127.0.0.1:5005"
  echo "gateway health: $(curl -fsS http://127.0.0.1:5005/gateway/health 2>&1 | head -1)   files: $(curl -fsS http://127.0.0.1:8089/gateway/health 2>&1 | head -1)"
  echo "api   (through $api): $(curl -fsS "$api/health/status" 2>&1 | head -c 300)"
  echo "worker ops: $(curl -fsS http://127.0.0.1:8082/health/status 2>&1 | head -c 300)"
  echo "pyworker: $(curl -fsS http://127.0.0.1:9100/readyz 2>&1 | head -c 300)"
}

p7_down() {
  local n
  for n in nginx worker api; do [ -f "$S7/$n.pid" ] && kill -TERM "$(cat "$S7/$n.pid")" 2>/dev/null; done
  sleep 2
  for n in pyworker valkey mirror nginx worker api; do [ -f "$S7/$n.pid" ] && { kill "$(cat "$S7/$n.pid")" 2>/dev/null; rm -f "$S7/$n.pid"; }; done
  echo "stack stopped"
}

p7_request() { # p7_request ROUTE JSON  -> posts through the gateway, prints the task id
  curl -fsS -X POST "http://127.0.0.1:${API_PORT:-5005}/$1" -H 'content-type: application/json' -d "$2" | python3 -c 'import json,sys; print(json.load(sys.stdin)["task_id"])'
}

p7_wait() { # p7_wait TASK_ID -> prints each new state, returns 0 on SUCCESS
  local last="" s
  for _ in $(seq 1 1200); do
    s=$(curl -sS "http://127.0.0.1:${API_PORT:-5005}/task_status/$1")
    [ "$s" != "$last" ] && { echo "$s"; last="$s"; }
    case "$s" in *'"SUCCESS"'*) return 0;; *'"FAILURE"'*) return 1;; esac
    sleep 0.25
  done
  return 2
}
