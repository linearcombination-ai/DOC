"""
Pin the Celery baseline configuration (no broker, backend or docker needed).
"""

import importlib
import os
from pathlib import Path
from types import ModuleType
from typing import Iterator

import pytest
from celery.exceptions import SoftTimeLimitExceeded
from doc.domain import celeryconfig
from doc.domain.task_limits import (
    DocumentGenerationTimeoutError,
    fail_cleanly_on_soft_time_limit,
)
from doc.domain.worker import app

ENV_VARS = [
    "CELERY_TASK_SOFT_TIME_LIMIT",
    "CELERY_TASK_TIME_LIMIT",
    "CELERY_WORKER_PREFETCH_MULTIPLIER",
    "CELERY_WORKER_MAX_TASKS_PER_CHILD",
    "CELERY_RESULT_EXPIRES",
    "CELERY_PUBLISH_MAX_RETRIES",
    "CELERY_PUBLISH_INTERVAL_START",
    "CELERY_PUBLISH_INTERVAL_STEP",
    "CELERY_PUBLISH_INTERVAL_MAX",
    "CELERY_RESULT_BACKEND_MAX_RETRIES",
]


@pytest.fixture
def clean_env(monkeypatch: pytest.MonkeyPatch) -> Iterator[pytest.MonkeyPatch]:
    """Remove tunables so defaults apply; restore the module afterwards."""
    for name in ENV_VARS:
        monkeypatch.delenv(name, raising=False)
    yield monkeypatch
    monkeypatch.undo()
    importlib.reload(celeryconfig)


def reload_config() -> ModuleType:
    return importlib.reload(celeryconfig)


def test_serialization_is_explicit_json() -> None:
    assert app.conf.task_serializer == "json"
    assert app.conf.result_serializer == "json"
    assert list(app.conf.accept_content) == ["json"]


def test_default_time_limits_soft_below_hard(clean_env: pytest.MonkeyPatch) -> None:
    config = reload_config()
    assert config.task_soft_time_limit == 20 * 60
    assert config.task_time_limit == 25 * 60
    assert config.task_soft_time_limit < config.task_time_limit


def test_hard_limit_is_forced_above_soft_limit(
    clean_env: pytest.MonkeyPatch,
) -> None:
    clean_env.setenv("CELERY_TASK_SOFT_TIME_LIMIT", "600")
    clean_env.setenv("CELERY_TASK_TIME_LIMIT", "300")
    config = reload_config()
    assert config.task_soft_time_limit == 600
    assert config.task_time_limit > config.task_soft_time_limit


def test_worker_settings(clean_env: pytest.MonkeyPatch) -> None:
    config = reload_config()
    assert config.worker_prefetch_multiplier == 1
    assert config.worker_max_tasks_per_child == 25
    assert config.task_track_started is True
    assert config.task_send_sent_event is True
    assert config.result_expires == 6 * 60 * 60


def test_connection_self_healing_settings(clean_env: pytest.MonkeyPatch) -> None:
    config = reload_config()
    assert config.broker_connection_retry_on_startup is True
    assert config.broker_connection_max_retries is None
    assert config.worker_cancel_long_running_tasks_on_connection_loss is False
    assert config.result_backend_always_retry is True
    assert config.result_backend_max_retries == 10
    assert config.redis_socket_keepalive is True
    assert config.redis_retry_on_timeout is True
    assert config.broker_transport_options == {
        "socket_keepalive": True,
        "retry_on_timeout": True,
    }


def test_publish_retry_policy_is_patient(clean_env: pytest.MonkeyPatch) -> None:
    policy: dict[str, object] = reload_config().task_publish_retry_policy
    assert policy == {
        "max_retries": 10,
        "interval_start": 0.2,
        "interval_step": 0.5,
        "interval_max": 3.0,
    }
    # Celery's default was max_retries=3 with intervals under 1 second.
    max_retries = policy["max_retries"]
    assert isinstance(max_retries, int) and max_retries > 3


def test_environment_overrides(clean_env: pytest.MonkeyPatch) -> None:
    clean_env.setenv("CELERY_TASK_SOFT_TIME_LIMIT", "100")
    clean_env.setenv("CELERY_TASK_TIME_LIMIT", "200")
    clean_env.setenv("CELERY_WORKER_MAX_TASKS_PER_CHILD", "5")
    clean_env.setenv("CELERY_RESULT_EXPIRES", "60")
    clean_env.setenv("CELERY_PUBLISH_MAX_RETRIES", "2")
    config = reload_config()
    assert config.task_soft_time_limit == 100
    assert config.task_time_limit == 200
    assert config.worker_max_tasks_per_child == 5
    assert config.result_expires == 60
    assert config.task_publish_retry_policy["max_retries"] == 2


def test_empty_environment_value_uses_default(
    clean_env: pytest.MonkeyPatch,
) -> None:
    clean_env.setenv("CELERY_TASK_SOFT_TIME_LIMIT", "")
    assert reload_config().task_soft_time_limit == 20 * 60


def test_delivery_semantics_are_unchanged(clean_env: pytest.MonkeyPatch) -> None:
    """acks_late and visibility_timeout belong to a later, higher-risk change."""
    config = reload_config()
    assert not hasattr(config, "task_acks_late")
    assert "visibility_timeout" not in config.broker_transport_options
    assert app.conf.task_acks_late is False


def test_app_picks_up_baseline_config() -> None:
    assert app.conf.worker_prefetch_multiplier == 1
    assert app.conf.task_track_started is True
    assert app.conf.broker_connection_retry_on_startup is True
    assert app.conf.task_soft_time_limit < app.conf.task_time_limit


def test_soft_time_limit_removes_partial_outputs_and_fails_cleanly(
    tmp_path: Path,
) -> None:
    partial = tmp_path / "partial.pdf"
    with pytest.raises(DocumentGenerationTimeoutError, match="took too long"):
        with fail_cleanly_on_soft_time_limit([str(partial)]):
            partial.write_bytes(b"%PDF-truncated")
            raise SoftTimeLimitExceeded()
    assert not partial.exists()


def test_soft_time_limit_keeps_fresh_cached_outputs(tmp_path: Path) -> None:
    fresh = tmp_path / "fresh.html"
    fresh.write_text("<html></html>")
    missing = tmp_path / "missing.pdf"
    with pytest.raises(DocumentGenerationTimeoutError):
        with fail_cleanly_on_soft_time_limit([str(fresh), str(missing)]):
            missing.write_bytes(b"partial")
            raise SoftTimeLimitExceeded()
    assert fresh.exists()
    assert not missing.exists()


def test_other_exceptions_pass_through(tmp_path: Path) -> None:
    out = tmp_path / "out.docx"
    out.write_bytes(b"x")
    with pytest.raises(ValueError):
        with fail_cleanly_on_soft_time_limit([str(out)]):
            raise ValueError("boom")
    assert out.exists()


def test_no_exception_leaves_outputs_alone(tmp_path: Path) -> None:
    out = tmp_path / "out.docx"
    with fail_cleanly_on_soft_time_limit([str(out)]):
        out.write_bytes(b"complete")
    assert out.read_bytes() == b"complete"
    assert os.path.exists(out)


def test_soft_time_limit_keeps_outputs_marked_complete(tmp_path: Path) -> None:
    done = tmp_path / "done.docx"
    partial = tmp_path / "partial.pdf"
    with pytest.raises(DocumentGenerationTimeoutError):
        with fail_cleanly_on_soft_time_limit([str(done), str(partial)]) as guard:
            done.write_bytes(b"complete")
            guard.mark_complete(str(done))
            partial.write_bytes(b"partial")
            raise SoftTimeLimitExceeded()
    assert done.read_bytes() == b"complete"
    assert not partial.exists()
