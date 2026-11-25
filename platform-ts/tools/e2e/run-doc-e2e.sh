#!/usr/bin/env bash
# Run DOC's own API end-to-end tests (tests/e2e/doc/test_api.py) against a running service and print a one-line-per-test result.
#   bash tools/e2e/run-doc-e2e.sh <label> <base url> <service output dir> [pytest -k expression]
# Needs DOC's venv (pytest and DOC's dependencies; DOC_VENV=...), run from DOC's repository root (the tests read ./.env).
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
REPO="$(cd "$HERE/.." && pwd)"
LABEL="$1"; URL="$2"; OUT="$3"; K="${4:-}"
VENV="${DOC_VENV:-$REPO/_exp/venv}"
mkdir -p "$HERE/results"
cd "$REPO"
# DOC's settings object reads ./.env itself; the variables below override it for this run
export E2E_BASE_URL="$URL" DOCUMENT_OUTPUT_DIR="$OUT" PYTHONPATH="$HERE/tools/e2e:$REPO/backend:$REPO" PYTHONDONTWRITEBYTECODE=1
export API_LOCAL_PORT="${URL##*:}" E2E_TIMEOUT_SECONDS="${E2E_TIMEOUT_SECONDS:-120}"
ARGS=(-p doc_e2e_shim -p no:cacheprovider -o addopts="" -rA --tb=line -q --durations=0 tests/e2e/doc/test_api.py)
[ -n "$K" ] && ARGS+=(-k "$K")
"$VENV/bin/python" -m pytest "${ARGS[@]}" 2>&1 | tee "$HERE/results/e2e-$LABEL.txt" | grep -E "^(PASSED|FAILED|ERROR|SKIPPED)|passed|failed" | sed 's/ - .*//' 
