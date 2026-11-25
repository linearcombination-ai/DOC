#!/usr/bin/env bash
# Re-run the P5 apalis-redis questions against the spike in spikes/apalis (apalis-redis pinned in its Cargo.toml).
set -u
: "${P6:?source env.sh}"; cd "$P6/spikes/apalis"
cargo build -q 2>&1 | tail -2
BIN="$CARGO_TARGET_DIR/debug/apalis-spike"; [ -x "$BIN" ] || BIN=$(ls target/debug/apalis-spike)
R="${REDIS_BIN:-$P6/.p6env/redis/bin/redis-server}"; CLI="$(dirname "$R")/redis-cli"; PORT=6390
run_redis() { rm -rf "$P6/scratch/apalis-r"; mkdir -p "$P6/scratch/apalis-r"; "$R" --port $PORT --bind 127.0.0.1 --dir "$P6/scratch/apalis-r" --save "" > /dev/null 2>&1 & RP=$!; sleep 0.6; }
stop_redis() { kill $RP 2>/dev/null; wait $RP 2>/dev/null; }
echo "== E1: default config (idempotency_ttl None): two pushes of the same key"
run_redis; "$BIN" push A; "$BIN" push A; $BIN work 300 & W=$!; sleep 2; kill $W 2>/dev/null; echo "keys: $($CLI -p $PORT keys '*idempot*')"; stop_redis
echo "== E1b: idempotency_ttl=3600: push, finish, push again"
run_redis; IDEM_TTL=3600 "$BIN" push A; IDEM_TTL=3600 "$BIN" work 300 & W=$!; sleep 2; IDEM_TTL=3600 "$BIN" push A; sleep 2; kill $W 2>/dev/null; echo "ttl: $($CLI -p $PORT ttl spike:idempotency:A)"; stop_redis
echo "== E2: SIGKILL a worker mid-job (heartbeat 1 s, missed 2)"
run_redis; "$BIN" push B; "$BIN" work 20000 & W1=$!; sleep 1.5; kill -9 $W1; T=$(date +%s.%N); "$BIN" work 300 > "$P6/scratch/apalis-w2.out" 2>&1 & W2=$!
for _ in $(seq 1 100); do grep -q "done B" "$P6/scratch/apalis-w2.out" && break; sleep 0.2; done; echo "second worker finished B $(python3 -c "import time;print(round(time.time()-$T,1))") s after the kill"; kill $W2 2>/dev/null; stop_redis
