"""
Background job plumbing, built on RQ.

The API process enqueues document generation jobs with ``enqueue`` and
reports on them with ``task_status``; a separate ``rq worker`` process
executes them. Progress is reported from inside a running job with
``update_state``.

``task_status`` deliberately keeps the response shape and state names
the frontend and tests already rely on: ``PENDING``, ``SUCCESS``,
``FAILURE``, ``REVOKED`` plus the free-form progress strings that jobs
publish through ``update_state``.
"""

import os
from typing import Any, Callable

from redis import Redis
from rq import Queue, get_current_job
from rq.exceptions import NoSuchJobError
from rq.job import Job, JobStatus

REDIS_URL = os.environ.get("REDIS_URL", "redis://")

# RQ's own defaults (180s job timeout, 500s result TTL) are far too
# short for document generation and for clients that poll for a result.
JOB_TIMEOUT_SECONDS = int(os.environ.get("JOB_TIMEOUT_SECONDS", "1800"))
RESULT_TTL_SECONDS = int(os.environ.get("RESULT_TTL_SECONDS", "86400"))
FAILURE_TTL_SECONDS = int(os.environ.get("FAILURE_TTL_SECONDS", "86400"))

# State names reported to clients.
PENDING = "PENDING"
SUCCESS = "SUCCESS"
FAILURE = "FAILURE"
REVOKED = "REVOKED"
STARTED = "STARTED"

# Connection is created lazily by redis-py so importing this module
# never touches the network.
connection: Redis = Redis.from_url(REDIS_URL)
queue = Queue(connection=connection)


def enqueue(func: Callable[..., Any], *args: Any) -> str:
    """Enqueue func(*args) to run on a worker and return the job id."""
    job = queue.enqueue(
        func,
        *args,
        job_timeout=JOB_TIMEOUT_SECONDS,
        result_ttl=RESULT_TTL_SECONDS,
        failure_ttl=FAILURE_TTL_SECONDS,
    )
    return job.id


def update_state(state: str) -> None:
    """
    Publish a progress state for the currently running job. Does
    nothing when called outside a worker, e.g., from doctests or when
    the generator functions are invoked directly.
    """
    job = get_current_job()
    if job is None:
        return
    job.meta["state"] = state
    job.save_meta()  # type: ignore[no-untyped-call]  # rq is untyped here


def task_status(task_id: str) -> dict[str, Any]:
    """
    Return {"state": ...} for the job, plus {"result": ...} when it has
    succeeded. Unknown ids report PENDING, as clients have always seen.
    """
    try:
        job = Job.fetch(task_id, connection=connection)
    except NoSuchJobError:
        return {"state": PENDING}
    status = job.get_status(refresh=True)
    if status == JobStatus.FINISHED:
        return {"state": SUCCESS, "result": job.return_value()}
    if status == JobStatus.FAILED:
        return {"state": FAILURE}
    if status in (JobStatus.CANCELED, JobStatus.STOPPED):
        return {"state": REVOKED}
    if status == JobStatus.STARTED:
        return {"state": job.meta.get("state", STARTED)}
    # queued, deferred, scheduled
    return {"state": PENDING}
