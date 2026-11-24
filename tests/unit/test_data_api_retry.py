import time

import pytest
import requests
from doc.domain import resource_lookup


def _response(status_code: int) -> requests.Response:
    response = requests.Response()
    response.status_code = status_code
    return response


def _patch_post(
    monkeypatch: pytest.MonkeyPatch,
    outcomes: list[requests.Response | Exception],
) -> list[int]:
    calls: list[int] = []

    def fake_post(
        url: str, json: dict[str, str], headers: dict[str, str]
    ) -> requests.Response:
        calls.append(1)
        outcome = outcomes.pop(0)
        if isinstance(outcome, Exception):
            raise outcome
        return outcome

    monkeypatch.setattr(requests, "post", fake_post)
    monkeypatch.setattr(time, "sleep", lambda _seconds: None)
    return calls


def test_retries_transient_connection_error_then_succeeds(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    ok = _response(200)
    calls = _patch_post(
        monkeypatch, [requests.exceptions.ConnectionError("reset"), ok]
    )
    response = resource_lookup._post_to_data_api("http://x", {"query": "q"}, {})
    assert response is ok
    assert len(calls) == 2


def test_retries_server_error_then_succeeds(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    ok = _response(200)
    calls = _patch_post(monkeypatch, [_response(502), ok])
    response = resource_lookup._post_to_data_api("http://x", {"query": "q"}, {})
    assert response is ok
    assert len(calls) == 2


def test_raises_after_exhausting_attempts(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    errors: list[requests.Response | Exception] = [
        requests.exceptions.ConnectionError("reset")
        for _ in range(resource_lookup.DATA_API_POST_ATTEMPTS)
    ]
    calls = _patch_post(monkeypatch, errors)
    with pytest.raises(requests.RequestException):
        resource_lookup._post_to_data_api("http://x", {"query": "q"}, {})
    assert len(calls) == resource_lookup.DATA_API_POST_ATTEMPTS


def test_client_error_is_not_retried(monkeypatch: pytest.MonkeyPatch) -> None:
    bad = _response(404)
    calls = _patch_post(monkeypatch, [bad])
    assert resource_lookup._post_to_data_api("http://x", {"query": "q"}, {}) is bad
    assert len(calls) == 1
