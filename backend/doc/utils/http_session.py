"""
A shared requests Session for calls to external HTTP services (the data
API and the USFM parser).

Every call made through this module has a (connect, read) timeout, and
connect errors and 502/503/504 responses are retried with backoff. The
requests we make (GraphQL queries and USFM conversion) are idempotent,
so retrying POSTs is safe.
"""

import threading
from typing import Mapping, Optional

import requests
from doc.config import settings
from requests.adapters import HTTPAdapter
from urllib3.util.retry import Retry

RETRY_STATUS_CODES = (502, 503, 504)

_session: Optional[requests.Session] = None
_session_lock = threading.Lock()


def request_timeout() -> tuple[float, float]:
    """The (connect, read) timeout applied to every external HTTP call."""
    return (settings.HTTP_CONNECT_TIMEOUT_SECONDS, settings.HTTP_READ_TIMEOUT_SECONDS)


def _build_session() -> requests.Session:
    retry = Retry(
        total=settings.HTTP_RETRY_TOTAL,
        connect=settings.HTTP_RETRY_TOTAL,
        read=0,  # A read timeout is not retried, it would multiply the wait.
        status=settings.HTTP_RETRY_TOTAL,
        status_forcelist=RETRY_STATUS_CODES,
        allowed_methods=frozenset({"GET", "POST"}),
        backoff_factor=settings.HTTP_RETRY_BACKOFF_FACTOR,
        # After retries are exhausted hand back the last response rather
        # than raising, callers inspect the status code themselves.
        raise_on_status=False,
    )
    adapter = HTTPAdapter(max_retries=retry)
    session = requests.Session()
    session.mount("http://", adapter)
    session.mount("https://", adapter)
    return session


def get_session() -> requests.Session:
    """Return the process-wide shared session, creating it on first use."""
    global _session
    if _session is None:
        with _session_lock:
            if _session is None:
                _session = _build_session()
    return _session


def post(
    url: str,
    *,
    json: Mapping[str, object],
    headers: Optional[Mapping[str, str]] = None,
    timeout: Optional[tuple[float, float]] = None,
) -> requests.Response:
    """POST via the shared session, always with a (connect, read) timeout."""
    return get_session().post(
        url,
        json=json,
        headers=headers,
        timeout=request_timeout() if timeout is None else timeout,
    )
