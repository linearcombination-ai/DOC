#!/usr/bin/env bash
# Print DOC's own Dockerfile (repository root) with two things removed, nothing else changed:
#   1. the Calibre installer and its `ebook-convert --version` test: ePub output is being removed upstream (PR 415) and the installer download
#      failed in CI on 2026-11-24, which kept DOC's own image from building;
#   2. the `mypy --strict` RUN lines (a type check of DOC's own code, not part of what the end-to-end tests compare).
# The result is used (a) as the image DOC's FastAPI + Celery baseline stack runs, and (b) as the test-runner image (DOC's tests import DOC's own
# `doc` package) for both stacks. Usage: bash ci/make-baseline-dockerfile.sh > /tmp/Dockerfile.baseline && docker build -f /tmp/Dockerfile.baseline <repo root>
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
sed \
  -e '/^# Get and install calibre/,/^RUN ebook-convert --version/d' \
  -e '/^RUN mypy --strict/d' \
  -e 's# /home/appuser/calibre-bin##' \
  "$HERE/Dockerfile"
