"""A pytest plugin that points DOC's own API tests (tests/e2e) at a running service.

DOC's tests build `fastapi.testclient.TestClient(app=app, base_url=...)`, which runs the FastAPI app in this process.
This plugin replaces that class with an httpx client aimed at `E2E_TARGET` (a URL), so the same unmodified tests run
against DOC's FastAPI + Celery stack, or against doc-jobd. The tests then read the finished file from
`settings.DOCUMENT_OUTPUT_DIR` (set it to the target's output directory).

    E2E_TARGET=http://127.0.0.1:8081 DOCUMENT_OUTPUT_DIR=/path/out PYTHONPATH=backend:p6rs/scripts/e2e \
        pytest -p e2e_remote tests/e2e/doc/test_api.py
"""

from __future__ import annotations

import os

import fastapi.testclient as testclient
import httpx


class RemoteClient(httpx.Client):
    """Stands in for TestClient: the `app` argument is ignored, requests go to E2E_TARGET."""

    def __init__(self, app: object = None, base_url: str = "", **kwargs: object) -> None:
        super().__init__(base_url=os.environ["E2E_TARGET"], timeout=120.0)


testclient.TestClient = RemoteClient  # type: ignore[misc,assignment]
