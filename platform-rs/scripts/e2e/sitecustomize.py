"""Loaded by the DOC baseline processes (PYTHONPATH): DOC calls its .NET USFM sidecar at http://usfmparserapi:80 (a
docker hostname on port 80). Here the sidecar listens on 127.0.0.1:5099, so the URL is rewritten. Nothing in DOC changes."""

from __future__ import annotations

import requests

_orig = requests.sessions.Session.request


def _patched(self: requests.sessions.Session, method: str, url: str, *args: object, **kwargs: object) -> requests.Response:
    return _orig(self, method, url.replace("http://usfmparserapi:80", "http://127.0.0.1:5099"), *args, **kwargs)  # type: ignore[arg-type]


requests.sessions.Session.request = _patched  # type: ignore[method-assign,assignment]
