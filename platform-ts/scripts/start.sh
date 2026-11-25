#!/usr/bin/env bash
# Dockerfile-free local run: builds the wasm core if it is missing, then starts the shell.
# Configuration comes from the environment (see src/config.ts); sensible defaults for a local run:
#   DATA_API_URL=https://api.bibleineverylanguage.org/v1/graphql  PORT=18301  DOC_MODE=as_built
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$HERE"
[ -f core/pkg/doc_wasm.js ] || bash scripts/build-wasm.sh
[ -d node_modules ] || npm ci --no-audit --no-fund
export DATA_API_URL="${DATA_API_URL:-https://api.bibleineverylanguage.org/v1/graphql}"
exec node src/main.ts
