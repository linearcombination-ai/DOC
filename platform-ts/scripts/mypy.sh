#!/usr/bin/env bash
# mypy --strict over every Python file of this prototype (the docx seam and the generator). Needs mypy (DOC's venv has it): MYPY=/path/to/mypy.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$HERE"
MYPY="${MYPY:-mypy}"
MYPYPATH="python:python/docxgen" "$MYPY" --strict --python-version 3.12 python/docx_core.py python/docx_worker.py python/docx_server.py python/docxgen/*.py tools/*.py tools/e2e/*.py
