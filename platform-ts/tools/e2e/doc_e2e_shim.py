"""pytest plugin, not DOC code: points DOC's own API end-to-end tests at a running service instead of FastAPI's in-process test client.

DOC's tests do `with TestClient(app=app, base_url=settings.api_test_url()) as client: client.post(...)`. `TestClient(app=...)` talks to
the app object in the same process and ignores the URL. This plugin replaces `fastapi.testclient.TestClient` with a thin httpx client that
talks to E2E_BASE_URL, so the unmodified tests drive whichever service listens there (DOC's own FastAPI + Celery, or this build).
The tests then look for the finished document in DOCUMENT_OUTPUT_DIR (set it to the directory the service under test writes to).

    PYTHONPATH=<this dir> E2E_BASE_URL=http://127.0.0.1:18301 DOCUMENT_OUTPUT_DIR=<service output dir> pytest -p doc_e2e_shim tests/e2e/doc/test_api.py

mypy --strict clean, no typing.Any.
"""

from __future__ import annotations

import os
import fastapi.testclient
import httpx


class RemoteClient(httpx.Client):
    """Same call shape as fastapi's TestClient for what the tests use: context manager, .get, .post with json=."""

    def __init__(self, app: object = None, base_url: str = "", **kwargs: object) -> None:
        del app  # the point of the shim: the app object is not used, the URL is
        super().__init__(base_url=os.environ["E2E_BASE_URL"], timeout=float(os.environ.get("E2E_TIMEOUT_SECONDS", "120")))
        del kwargs, base_url


def pytest_configure() -> None:
    fastapi.testclient.TestClient = RemoteClient  # type: ignore[misc, assignment]
