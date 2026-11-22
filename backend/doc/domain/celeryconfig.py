import os


def _int_env(name: str, default: int) -> int:
    """Read an integer setting from the environment, falling back to default."""
    raw = os.environ.get(name)
    if raw is None or raw.strip() == "":
        return default
    return int(raw)


def _float_env(name: str, default: float) -> float:
    """Read a float setting from the environment, falling back to default."""
    raw = os.environ.get(name)
    if raw is None or raw.strip() == "":
        return default
    return float(raw)


## Broker settings.
broker_url = os.environ.get("CELERY_BROKER_URL", "redis://")

## Using the database to store task state and results.
result_backend = os.environ.get("CELERY_RESULT_BACKEND", "redis://")

# List of modules to import when the Celery worker starts.
imports = (
    "doc.domain.resource_lookup",
    "doc.domain.document_generator",
    "passages.domain.document_generator",
    "stet.domain.document_generator",
)


## Serialization: make the current Celery defaults explicit. Task arguments and
## results are primitives or JSON strings, so JSON is sufficient and safe.
task_serializer = "json"
result_serializer = "json"
accept_content = ["json"]

## Time limits. A stalled git clone, HTTP call, SMTP connect or converter
## subprocess would otherwise occupy a pool child forever. The soft limit raises
## SoftTimeLimitExceeded inside the task (which the generation tasks handle by
## cleaning up partial outputs); the hard limit kills the child as a backstop.
## Defaults are generous; tune without an image rebuild via the environment.
task_soft_time_limit = _int_env("CELERY_TASK_SOFT_TIME_LIMIT", 20 * 60)
task_time_limit = _int_env("CELERY_TASK_TIME_LIMIT", 25 * 60)
if task_time_limit <= task_soft_time_limit:
    # The soft limit must fire first or it can never run its cleanup.
    task_time_limit = task_soft_time_limit + 60

## Worker behavior for minutes-long, memory-hungry tasks.
# Reserve only one task per pool child so queued work is not stuck behind a
# long-running task on a busy child.
worker_prefetch_multiplier = _int_env("CELERY_WORKER_PREFETCH_MULTIPLIER", 1)
# Recycle pool children periodically to bound memory growth (WeasyPrint etc.).
worker_max_tasks_per_child = _int_env("CELERY_WORKER_MAX_TASKS_PER_CHILD", 25)

## Observability: distinguish "queued" from "running" and let Flower see "sent".
task_track_started = True
task_send_sent_event = True

## Result retention.
result_expires = _int_env("CELERY_RESULT_EXPIRES", 6 * 60 * 60)

## Self-healing connection behavior.
# Keep retrying the broker connection at startup (default flips in Celery 6.0).
broker_connection_retry_on_startup = True
# Wait indefinitely for the broker instead of exiting after 100 attempts.
broker_connection_max_retries = None
# Explicit: do not cancel running tasks when the broker connection drops.
worker_cancel_long_running_tasks_on_connection_loss = False

# Be patient when publishing a task during a broker blip (roughly 20 s in total
# with the defaults) rather than failing the user's request after under a second.
task_publish_retry_policy: dict[str, object] = {
    "max_retries": _int_env("CELERY_PUBLISH_MAX_RETRIES", 10),
    "interval_start": _float_env("CELERY_PUBLISH_INTERVAL_START", 0.2),
    "interval_step": _float_env("CELERY_PUBLISH_INTERVAL_STEP", 0.5),
    "interval_max": _float_env("CELERY_PUBLISH_INTERVAL_MAX", 3.0),
}

# Retry result backend operations on transient errors, with a bounded count.
result_backend_always_retry = True
result_backend_max_retries = _int_env("CELERY_RESULT_BACKEND_MAX_RETRIES", 10)

# Redis connection health for both the broker transport and the result backend.
broker_transport_options: dict[str, object] = {
    "socket_keepalive": True,
    "retry_on_timeout": True,
}
redis_socket_keepalive = True
redis_retry_on_timeout = True

# task_annotations = {"document.domain.document_generator.main": {"rate_limit": "10/s"}}

# task_queues = {
#     'test-queue': {
#         'exchange': 'test-queue',
#     }
# }
# task_routes = {
#     "worker.celery_worker.long_task": "test-queue"
# }

# worker_concurrency = 1
