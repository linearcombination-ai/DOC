"""
Tests of tests.shared.utils.check_result, the e2e task status poll helper:
it must stop on every terminal state and must not poll forever.
"""

import dataclasses
import logging
from collections.abc import Sequence
from pathlib import Path
from types import TracebackType
from typing import Optional

import httpx
import pytest

from tests.shared import utils
from tests.unit.celery.celery_scaffold import JsonValue

TASK_ID = "11111111-2222-3333-4444-555555555555"


@dataclasses.dataclass
class FakeClock:
    """Replaces the time module inside utils, never really sleeping."""

    now: float = 0.0
    sleeps: list[float] = dataclasses.field(default_factory=list)

    def monotonic(self) -> float:
        return self.now

    def sleep(self, seconds: float) -> None:
        self.sleeps.append(seconds)
        self.now += seconds


@dataclasses.dataclass
class FakeSettings:
    """Replaces the pydantic (frozen) settings inside utils."""

    DOCUMENT_OUTPUT_DIR: str

    def api_test_url(self) -> str:
        return "http://test"

    def logger(self, name: str) -> logging.Logger:
        return logging.getLogger(name)


@dataclasses.dataclass
class FakeStatusResponse:
    payload: dict[str, JsonValue]
    status_code: int = 200

    def json(self) -> dict[str, JsonValue]:
        return self.payload


@dataclasses.dataclass
class FakeStatusClient:
    """
    Replaces TestClient inside utils. Answers each status request with
    the next scripted payload, repeating the last one when exhausted.
    """

    payloads: Sequence[dict[str, JsonValue]]
    urls: list[str] = dataclasses.field(default_factory=list)

    def __call__(self, **kwargs: object) -> "FakeStatusClient":
        return self

    def __enter__(self) -> "FakeStatusClient":
        return self

    def __exit__(
        self,
        exc_type: Optional[type[BaseException]],
        exc: Optional[BaseException],
        tb: Optional[TracebackType],
    ) -> None:
        return None

    def get(self, url: str) -> FakeStatusResponse:
        index = min(len(self.urls), len(self.payloads) - 1)
        self.urls.append(url)
        return FakeStatusResponse(self.payloads[index])


def install(
    monkeypatch: pytest.MonkeyPatch, payloads: Sequence[dict[str, JsonValue]]
) -> tuple[FakeStatusClient, FakeClock]:
    client = FakeStatusClient(payloads)
    clock = FakeClock()
    monkeypatch.setattr(utils, "TestClient", client)
    monkeypatch.setattr(utils, "time", clock)
    return client, clock


def submitted() -> httpx.Response:
    return httpx.Response(200, json={"task_id": TASK_ID})


def test_returns_key_when_task_succeeds_after_progress_states(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    monkeypatch.setattr(utils, "settings", FakeSettings(str(tmp_path)))
    (tmp_path / "the_key.html").write_text("<html></html>")
    client, clock = install(
        monkeypatch,
        [
            {"state": "PENDING"},
            {"state": "Assembling content"},
            {"state": "SUCCESS", "result": "the_key"},
        ],
    )
    assert utils.check_result(submitted(), "html", poll_duration=4) == "the_key"
    assert client.urls == [f"/task_status/{TASK_ID}"] * 3
    assert clock.sleeps == [4, 4]


def test_custom_status_url_and_states_are_honored(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    monkeypatch.setattr(utils, "settings", FakeSettings(str(tmp_path)))
    (tmp_path / "k.docx").write_text("x")
    client, _ = install(monkeypatch, [{"state": "DONE", "result": "k"}])
    key = utils.check_result(
        submitted(),
        "docx",
        status_url_fmt_str="/stet/task_status/{}",
        success_state="DONE",
    )
    assert key == "k"
    assert client.urls == [f"/stet/task_status/{TASK_ID}"]


def test_failure_state_fails_immediately(monkeypatch: pytest.MonkeyPatch) -> None:
    _, clock = install(monkeypatch, [{"state": "FAILURE"}])
    with pytest.raises(Exception, match="FAILURE"):
        utils.check_result(submitted(), "html")
    assert clock.sleeps == []


def test_revoked_state_fails_immediately(monkeypatch: pytest.MonkeyPatch) -> None:
    _, clock = install(monkeypatch, [{"state": "PENDING"}, {"state": "REVOKED"}])
    with pytest.raises(Exception, match="REVOKED"):
        utils.check_result(submitted(), "html")
    assert len(clock.sleeps) == 1


def test_task_stuck_in_pending_times_out_with_a_clear_message(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    client, clock = install(monkeypatch, [{"state": "PENDING"}])
    with pytest.raises(TimeoutError) as excinfo:
        utils.check_result(submitted(), "html", poll_duration=4, timeout_seconds=30)
    message = str(excinfo.value)
    assert TASK_ID in message
    assert "30" in message
    assert "PENDING" in message
    assert "worker" in message
    # Bounded: polled for about the deadline, not forever.
    assert clock.now >= 30
    assert len(client.urls) <= 30 // 4 + 2


def test_task_stuck_in_a_progress_state_times_out_naming_that_state(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    install(monkeypatch, [{"state": "Parsing asset files"}])
    with pytest.raises(TimeoutError, match="Parsing asset files"):
        utils.check_result(submitted(), "html", timeout_seconds=10)


def test_default_deadline_is_below_the_ci_step_timeout() -> None:
    assert 0 < utils.DEFAULT_TASK_TIMEOUT_SECONDS < 20 * 60
