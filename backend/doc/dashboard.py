"""
Read-only RQ dashboard.

rq-dashboard's own ``--disable-delete`` flag only blocks deleting jobs
and emptying queues. Requeueing and compacting remain available, so
this wrapper rejects every request that could change queue state and
leaves just the pages and JSON endpoints that display it.

Serve with, e.g.: gunicorn --bind 0.0.0.0:9181 doc.dashboard:app
"""

import os

from flask import Flask, Response, request
from rq_dashboard.cli import make_flask_app
from rq_dashboard.web import setup_rq_connection

READ_ONLY_METHODS = ("GET", "HEAD", "OPTIONS")
# Mutating despite being reachable with GET.
MUTATING_GET_PREFIXES = ("/requeue/",)


def create_app() -> Flask:
    app = make_flask_app(
        config=None,
        username=os.environ.get("RQ_DASHBOARD_USERNAME") or None,
        password=os.environ.get("RQ_DASHBOARD_PASSWORD") or None,
        url_prefix="",
    )
    app.config["RQ_DASHBOARD_REDIS_URL"] = [
        os.environ.get("REDIS_URL", "redis://")
    ]
    # Belt and braces; the guard below already covers these routes.
    app.config["RQ_DASHBOARD_DISABLE_DELETE"] = True

    @app.before_request
    def reject_state_changes() -> Response | None:
        if request.method not in READ_ONLY_METHODS or request.path.startswith(
            MUTATING_GET_PREFIXES
        ):
            return Response("Dashboard is read-only", 403)
        return None

    setup_rq_connection(app)
    return app


app = create_app()
