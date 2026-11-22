"""
Unit tests for external-call hygiene: timeouts on every external call,
failed/empty data API responses never cached, stale-on-error, and git
clone failures that are not swallowed. Uses its own fakes; no network,
git, docker or sleeping.
"""

import subprocess
import time
from typing import Any

import pytest
import requests
from doc.config import settings
from doc.domain import document_generator, email_utils, parsing, resource_lookup
from doc.domain.model import SourceData
from doc.utils import http_session, process_utils
from fastapi import HTTPException
from urllib3.util.retry import Retry

GOOD_PAYLOAD = {
    "data": {
        "git_repo": [
            {
                "repo_url": "https://example.com/org/en_ulb",
                "content": {
                    "resource_type": "ulb",
                    "language": {
                        "english_name": "English",
                        "ietf_code": "en",
                        "national_name": "English",
                        "direction": "ltr",
                    },
                },
            }
        ]
    }
}
URL_ARGS = ("https://api.example.com/graphql", "ua", "xrw")


class FakeResponse:
    def __init__(self, status_code: int = 200, payload: Any = None) -> None:
        self.status_code = status_code
        self._payload = payload

    def json(self) -> Any:
        if self._payload is None:
            raise ValueError("no json")
        return self._payload


class FakePost:
    """Scripted replacement for http_session.post."""

    def __init__(self, *outcomes: Any) -> None:
        self.outcomes = list(outcomes)
        self.calls: list[tuple[tuple[Any, ...], dict[str, Any]]] = []

    def __call__(self, *args: Any, **kwargs: Any) -> Any:
        self.calls.append((args, kwargs))
        outcome = self.outcomes.pop(0) if len(self.outcomes) > 1 else self.outcomes[0]
        if isinstance(outcome, Exception):
            raise outcome
        return outcome


@pytest.fixture(autouse=True)
def clean_caches() -> Any:
    def clear() -> None:
        for cache in (
            resource_lookup.fetch_source_data_cache,
            resource_lookup.fetch_non_primary_source_data_cache,
        ):
            cache.clear()
        resource_lookup.fetch_source_data_last_good.clear()
        resource_lookup.fetch_non_primary_source_data_last_good.clear()

    clear()
    yield
    clear()


# ---- fetch_source_data: not cached on failure, stale-on-error ----------


@pytest.mark.parametrize(
    "bad",
    [
        requests.ConnectionError("down"),
        FakeResponse(500),
        FakeResponse(200, {"data": {}}),  # no git_repo
        FakeResponse(200, {"data": {"git_repo": []}}),  # empty
        FakeResponse(200, None),  # not json
    ],
)
def test_failed_or_empty_response_is_not_cached(
    monkeypatch: pytest.MonkeyPatch, bad: Any
) -> None:
    fake = FakePost(bad, FakeResponse(200, GOOD_PAYLOAD))
    monkeypatch.setattr(http_session, "post", fake)
    first = resource_lookup.fetch_source_data(*URL_ARGS)  # type: ignore[arg-type]
    assert first is not None and first.git_repo == []
    # The API has recovered: the very next call must reach it, not a cache.
    second = resource_lookup.fetch_source_data(*URL_ARGS)  # type: ignore[arg-type]
    assert second is not None and len(second.git_repo) == 1
    assert len(fake.calls) == 2


def test_good_response_is_cached(monkeypatch: pytest.MonkeyPatch) -> None:
    fake = FakePost(FakeResponse(200, GOOD_PAYLOAD))
    monkeypatch.setattr(http_session, "post", fake)
    resource_lookup.fetch_source_data(*URL_ARGS)  # type: ignore[arg-type]
    resource_lookup.fetch_source_data(*URL_ARGS)  # type: ignore[arg-type]
    assert len(fake.calls) == 1


def test_stale_on_error_serves_last_good(monkeypatch: pytest.MonkeyPatch) -> None:
    fake = FakePost(FakeResponse(200, GOOD_PAYLOAD), requests.Timeout("slow"))
    monkeypatch.setattr(http_session, "post", fake)
    good = resource_lookup.fetch_source_data(*URL_ARGS)  # type: ignore[arg-type]
    assert good is not None and len(good.git_repo) == 1
    resource_lookup.fetch_source_data_cache.clear()  # TTL expiry
    stale = resource_lookup.fetch_source_data(*URL_ARGS)  # type: ignore[arg-type]
    assert stale == good
    assert len(fake.calls) == 2


def test_non_primary_failure_not_cached_and_stale_on_error(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    payload = {
        "data": {
            "git_repo": [
                {
                    **GOOD_PAYLOAD["data"]["git_repo"][0],  # type: ignore[dict-item]
                    "repo_url": "https://example.com/WA-Catalog/bg_ulb",
                }
            ]
        }
    }
    fake = FakePost(
        FakeResponse(503), FakeResponse(200, payload), requests.ConnectionError("x")
    )
    monkeypatch.setattr(http_session, "post", fake)
    fetch = resource_lookup.fetch_non_primary_source_data
    assert fetch(*URL_ARGS).git_repo == []  # type: ignore[arg-type,union-attr]
    good = fetch(*URL_ARGS)  # type: ignore[arg-type]
    assert good is not None and len(good.git_repo) == 1
    resource_lookup.fetch_non_primary_source_data_cache.clear()
    assert fetch(*URL_ARGS) == good  # type: ignore[arg-type]


def test_cached_decorators_have_locks() -> None:
    # cachetools exposes lock via the wrapper closure; assert it is wired.
    assert resource_lookup.fetch_source_data_lock.acquire(blocking=False)
    resource_lookup.fetch_source_data_lock.release()
    for fn in (
        resource_lookup._fetch_source_data_cached,
        resource_lookup._fetch_non_primary_source_data_cached,
    ):
        assert hasattr(fn, "cache_lock")


# ---- HTTP session ------------------------------------------------------


def test_http_post_passes_timeout(monkeypatch: pytest.MonkeyPatch) -> None:
    seen: dict[str, Any] = {}

    class FakeSession:
        def post(self, url: str, **kwargs: Any) -> str:
            seen.update(kwargs)
            return "ok"

    monkeypatch.setattr(http_session, "get_session", lambda: FakeSession())
    assert http_session.post("http://x", json={}) == "ok"
    assert seen["timeout"] == (
        settings.HTTP_CONNECT_TIMEOUT_SECONDS,
        settings.HTTP_READ_TIMEOUT_SECONDS,
    )


def test_session_retries_connect_errors_and_gateway_statuses() -> None:
    session = http_session._build_session()
    retry = session.get_adapter("https://example.com").max_retries
    assert isinstance(retry, Retry)
    assert set(retry.status_forcelist) == {502, 503, 504}
    assert retry.connect == settings.HTTP_RETRY_TOTAL
    assert retry.is_retry("POST", 503)
    assert not retry.is_retry("POST", 500)


def test_parser_post_goes_through_timeout_session(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    fake = FakePost(FakeResponse(200, {"ok": 1}))
    monkeypatch.setattr(http_session, "post", fake)
    parsing.convert_usfm_chapter_to_html("c", "in", "out", api_url="http://p/x")
    assert fake.calls[0][0] == ("http://p/x",)


# ---- run_with_retry / git clone ----------------------------------------


def completed(rc: int, stderr: str = "") -> subprocess.CompletedProcess[str]:
    return subprocess.CompletedProcess([], rc, "", stderr)


class FakeRun:
    def __init__(self, *outcomes: Any) -> None:
        self.outcomes = list(outcomes)
        self.calls: list[dict[str, Any]] = []

    def __call__(self, command: list[str], **kwargs: Any) -> Any:
        self.calls.append({"command": command, **kwargs})
        outcome = self.outcomes.pop(0) if len(self.outcomes) > 1 else self.outcomes[0]
        if isinstance(outcome, Exception):
            raise outcome
        return outcome


class Clock:
    """Fake monotonic clock advanced by fake sleeps and attempts."""

    def __init__(self) -> None:
        self.now = 0.0
        self.slept: list[float] = []

    def monotonic(self) -> float:
        return self.now

    def sleep(self, seconds: float) -> None:
        self.slept.append(seconds)
        self.now += seconds


def retry_kwargs(clock: Clock, run: FakeRun, deadline: float = 100.0) -> dict[str, Any]:
    return dict(
        per_attempt_timeout=30,
        deadline=deadline,
        wait_seconds=2.0,
        is_transient=process_utils.is_transient_git_failure,
        run=run,
        sleep=clock.sleep,
        monotonic=clock.monotonic,
    )


def test_clone_nonzero_exit_is_not_swallowed_permanent() -> None:
    clock = Clock()
    run = FakeRun(completed(128, "fatal: repository 'x' not found"))
    with pytest.raises(process_utils.CommandFailed) as exc:
        process_utils.run_with_retry(["git", "clone"], **retry_kwargs(clock, run))
    assert not exc.value.transient
    assert len(run.calls) == 1  # permanent: no pointless waiting
    assert clock.slept == []


def test_clone_transient_failure_is_retried_until_success() -> None:
    clock = Clock()
    cleaned: list[int] = []
    run = FakeRun(
        completed(128, "fatal: unable to access: Could not resolve host"),
        subprocess.TimeoutExpired("git", 30),
        completed(0),
    )
    result = process_utils.run_with_retry(
        ["git", "clone"],
        cleanup=lambda: cleaned.append(1),
        **retry_kwargs(clock, run),
    )
    assert result.returncode == 0
    assert len(run.calls) == 3
    assert len(clock.slept) == 2 and len(cleaned) == 2


def test_clone_transient_failure_gives_up_at_deadline() -> None:
    clock = Clock()
    run = FakeRun(completed(128, "early EOF"))
    with pytest.raises(process_utils.CommandFailed) as exc:
        process_utils.run_with_retry(
            ["git", "clone"], **retry_kwargs(clock, run, deadline=10.0)
        )
    assert exc.value.transient
    assert clock.now <= 10.0 + 1e-9  # bounded


def test_every_attempt_has_timeout_capped_by_deadline() -> None:
    clock = Clock()
    run = FakeRun(subprocess.TimeoutExpired("git", 1), completed(0))
    process_utils.run_with_retry(
        ["git", "clone"], **retry_kwargs(clock, run, deadline=35.0)
    )
    assert run.calls[0]["timeout"] == 30
    assert all(0 < call["timeout"] <= 30 for call in run.calls)


def test_curl_4xx_is_permanent_but_5xx_transient() -> None:
    f = process_utils.is_transient_curl_failure
    assert not f(completed(22, "curl: (22) The requested URL returned error: 404"))
    assert f(completed(22, "curl: (22) The requested URL returned error: 503"))
    assert f(completed(22, "curl: (22) The requested URL returned error: 429"))
    assert f(completed(28, "Operation timed out"))


def patch_process(
    monkeypatch: pytest.MonkeyPatch, run: FakeRun, sleep: Any = lambda s: None
) -> None:
    real = process_utils.run_with_retry

    def with_fakes(command: Any, **kwargs: Any) -> Any:
        kwargs.update(run=run, sleep=sleep)
        return real(command, **kwargs)

    monkeypatch.setattr(resource_lookup, "run_with_retry", with_fakes)


def test_clone_git_repo_raises_on_failure_and_uses_arg_list(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Any
) -> None:
    run = FakeRun(completed(128, "fatal: repository not found"))
    patch_process(monkeypatch, run)
    dest = str(tmp_path / "repo")
    with pytest.raises(HTTPException):
        resource_lookup.clone_git_repo(
            "https://example.com/o/r; touch /tmp/pwned",  # type: ignore[arg-type]
            dest,
            branch="main",
        )
    command = run.calls[0]["command"]
    assert isinstance(command, list)
    assert command[:3] == ["git", "clone", "--depth=1"]
    assert "--branch" in command and command[-1] == dest
    assert "shell" not in run.calls[0]
    assert run.calls[0]["timeout"] <= settings.GIT_CLONE_TIMEOUT_SECONDS


def test_clone_git_repo_success(monkeypatch: pytest.MonkeyPatch, tmp_path: Any) -> None:
    run = FakeRun(completed(0))
    patch_process(monkeypatch, run)
    resource_lookup.clone_git_repo(
        "https://example.com/o/r",  # type: ignore[arg-type]
        str(tmp_path / "repo"),
    )
    assert len(run.calls) == 1


def test_batch_clone_failure_is_raised_when_transient_exhausted(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Any
) -> None:
    run = FakeRun(completed(128, "Could not resolve host"))
    # Tiny real ceiling and real (short) waits: retries happen, then it gives up.
    patch_process(monkeypatch, run, sleep=lambda s: time.sleep(0.02))
    monkeypatch.setattr(
        resource_lookup, "deadline_from_now", lambda _: time.monotonic() + 0.1
    )
    with pytest.raises(HTTPException):
        resource_lookup.batch_clone_git_repos(
            [("https://example.com/o/r", str(tmp_path / "r"))],  # type: ignore[list-item]
            asset_caching_enabled=False,
        )
    assert len(run.calls) > 1  # it waited and retried before giving up


def test_batch_clone_permanent_failure_does_not_block_other_repos(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Any
) -> None:
    run = FakeRun(completed(128, "repository not found"), completed(0))
    patch_process(monkeypatch, run)
    resource_lookup.batch_clone_git_repos(
        [
            ("https://example.com/o/missing", str(tmp_path / "a")),  # type: ignore[list-item]
            ("https://example.com/o/ok", str(tmp_path / "b")),  # type: ignore[list-item]
        ],
        asset_caching_enabled=False,
    )
    assert len(run.calls) == 2
    assert all("||" not in " ".join(c["command"]) for c in run.calls)


# ---- timeouts on remaining subprocess/SMTP calls -----------------------


def test_converters_pass_timeout(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Any
) -> None:
    calls: list[dict[str, Any]] = []
    monkeypatch.setattr(
        subprocess, "run", lambda cmd, **kw: calls.append({"cmd": cmd, **kw})
    )
    html = tmp_path / "a.html"
    html.write_text("<html></html>")
    document_generator.convert_html_to_epub(str(html), str(tmp_path / "a.epub"), "key")
    for use_prince in (False, True):
        document_generator.convert_html_to_pdf(
            str(html), str(tmp_path / "a.pdf"), "key", use_prince
        )
    assert len(calls) == 3
    assert all(c["timeout"] == settings.CONVERTER_TIMEOUT_SECONDS for c in calls)
    assert all(isinstance(c["cmd"], list) for c in calls)


def test_smtp_gets_timeout(monkeypatch: pytest.MonkeyPatch) -> None:
    seen: dict[str, Any] = {}

    class FakeSMTP:
        def __init__(self, host: str, port: int, **kwargs: Any) -> None:
            seen.update(kwargs)
            raise OSError("stop here")  # email errors are swallowed by design

    monkeypatch.setattr(email_utils.smtplib, "SMTP", FakeSMTP)
    email_utils.send_email_with_attachment("a@example.com", [], "key")
    assert seen["timeout"] == settings.SMTP_TIMEOUT_SECONDS


def test_batch_download_uses_arg_lists_and_timeouts(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Any
) -> None:
    run = FakeRun(completed(0))
    patch_process(monkeypatch, run)
    unzip_calls: list[dict[str, Any]] = []
    monkeypatch.setattr(
        subprocess, "run", lambda cmd, **kw: unzip_calls.append({"cmd": cmd, **kw})
    )
    resource_lookup.batch_download_repos(
        [("https://example.com/o/r", str(tmp_path / "r"))],  # type: ignore[list-item]
        resource_assets_dir=str(tmp_path),
    )
    curl = run.calls[0]
    assert curl["command"][0] == "curl" and "--fail" in curl["command"]
    assert curl["timeout"] <= settings.DOWNLOAD_TIMEOUT_SECONDS
    assert unzip_calls[0]["timeout"] == settings.DOWNLOAD_TIMEOUT_SECONDS


def test_timeouts_below_celery_soft_limit() -> None:
    soft_limit = 20 * 60
    assert settings.GIT_CLONE_RETRY_CEILING_SECONDS < soft_limit
    assert settings.CONVERTER_TIMEOUT_SECONDS < soft_limit
    assert settings.DOWNLOAD_TIMEOUT_SECONDS < soft_limit
