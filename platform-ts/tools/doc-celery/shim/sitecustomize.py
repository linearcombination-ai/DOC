"""Lab shim, not DOC code: DOC calls the .NET USFM sidecar at http://usfmparserapi:80 (the docker-compose service name).
Without docker that host does not exist, so rewrite it to the locally started sidecar. Loaded through PYTHONPATH."""
import os

import requests

_TARGET = os.environ.get("USFM_SIDECAR_URL", "http://127.0.0.1:5099")
_orig = requests.sessions.Session.request


def _request(self: requests.sessions.Session, method: str, url: str, *args: object, **kwargs: object) -> requests.Response:
    return _orig(self, method, url.replace("http://usfmparserapi:80", _TARGET), *args, **kwargs)  # type: ignore[arg-type]


setattr(requests.sessions.Session, "request", _request)
