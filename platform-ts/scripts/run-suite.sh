#!/usr/bin/env bash
# Run the whole test suite (fault suite included) against one Redis-compatible server binary and keep the result.
#   bash scripts/run-suite.sh /path/to/redis-server redis8     ->  results/suite-redis8.txt
#   bash scripts/run-suite.sh /path/to/valkey-server valkey9  ->  results/suite-valkey9.txt
# The harness starts a fresh server per test file with `--appendonly yes --appendfsync everysec` (and a kill -9 test with AOF replay).
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="$1"; LABEL="$2"
cd "$HERE"
mkdir -p results
{
  echo "server: $("$BIN" --version)"
  echo "host load at start: $(cut -d' ' -f1-3 /proc/loadavg)"
  echo "date: $(date -u +%FT%TZ)"
  REDIS_SERVER_BIN="$BIN" npx vitest run --reporter=verbose 2>&1 | grep -E "^ *(✓|×|↓)|Test Files|Tests |Duration|TIMING|TEARDOWN" || true
} | tee "results/suite-$LABEL.txt"
