"""
Unit tests for external-call hygiene: timeouts on every external call,
failed/empty data API responses never cached, stale-on-error, and git
clone failures that are not swallowed. Uses its own fakes; no network,
git, docker or sleeping.
"""

import smtplib
import subprocess
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Callable, Iterator, Mapping, Optional, Sequence

import pytest
import requests
from doc.config import settings
from doc.domain import document_generator, email_utils, parsing, resource_lookup
from doc.domain.model import SourceData
from doc.utils import http_session, process_utils
from fastapi import HTTPException
from pydantic import HttpUrl
from requests.adapters import HTTPAdapter
from urllib3.util.retry import Retry


def repo_entry(url: str) -> dict[str, object]:
    return {
        "repo_url": url,
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


def payload_with(*urls: str) -> dict[str, object]:
    return {"data": {"git_repo": [repo_entry(url) for url in urls]}}


GOOD_PAYLOAD = payload_with("https://example.com/org/en_ulb")
API_URL = HttpUrl("https://api.example.com/graphql")


class FakeResponse:
    def __init__(self, status_code: int = 200, payload: object = None) -> None:
        self.status_code = status_code
        self._payload = payload

    def json(self) -> object:
        if self._payload is None:
            raise ValueError("no json")
        return self._payload


class FakePost:
    """Scripted replacement for http_session.post; records requested URLs."""

    def __init__(self, *outcomes: FakeResponse | Exception) -> None:
        self.outcomes = list(outcomes)
        self.urls: list[str] = []

    def __call__(
        self,
        url: str,
        *,
        json: Mapping[str, object],
        headers: Optional[Mapping[str, str]] = None,
    ) -> FakeResponse:
        self.urls.append(url)
        outcome = self.outcomes.pop(0) if len(self.outcomes) > 1 else self.outcomes[0]
        if isinstance(outcome, Exception):
            raise outcome
        return outcome


def fetch_primary() -> SourceData:
    result = resource_lookup.fetch_source_data(API_URL, "ua", "xrw")
    assert result is not None
    return result


def fetch_non_primary() -> SourceData:
    result = resource_lookup.fetch_non_primary_source_data(API_URL, "ua", "xrw")
    assert result is not None
    return result


@pytest.fixture(autouse=True)
def clean_caches() -> Iterator[None]:
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
    monkeypatch: pytest.MonkeyPatch, bad: FakeResponse | Exception
) -> None:
    fake = FakePost(bad, FakeResponse(200, GOOD_PAYLOAD))
    monkeypatch.setattr(http_session, "post", fake)
    assert fetch_primary().git_repo == []
    # The API has recovered: the very next call must reach it, not a cache.
    assert len(fetch_primary().git_repo) == 1
    assert len(fake.urls) == 2


def test_good_response_is_cached(monkeypatch: pytest.MonkeyPatch) -> None:
    fake = FakePost(FakeResponse(200, GOOD_PAYLOAD))
    monkeypatch.setattr(http_session, "post", fake)
    fetch_primary()
    fetch_primary()
    assert len(fake.urls) == 1


def test_stale_on_error_serves_last_good(monkeypatch: pytest.MonkeyPatch) -> None:
    fake = FakePost(FakeResponse(200, GOOD_PAYLOAD), requests.Timeout("slow"))
    monkeypatch.setattr(http_session, "post", fake)
    good = fetch_primary()
    assert len(good.git_repo) == 1
    resource_lookup.fetch_source_data_cache.clear()  # TTL expiry
    assert fetch_primary() == good
    assert len(fake.urls) == 2


def test_non_primary_failure_not_cached_and_stale_on_error(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    payload = payload_with("https://example.com/WA-Catalog/bg_ulb")
    fake = FakePost(
        FakeResponse(503), FakeResponse(200, payload), requests.ConnectionError("x")
    )
    monkeypatch.setattr(http_session, "post", fake)
    assert fetch_non_primary().git_repo == []
    good = fetch_non_primary()
    assert len(good.git_repo) == 1
    resource_lookup.fetch_non_primary_source_data_cache.clear()
    assert fetch_non_primary() == good


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
    seen: dict[str, object] = {}
    sentinel = requests.Response()

    class FakeSession:
        def post(
            self,
            url: str,
            *,
            json: Mapping[str, object],
            headers: Optional[Mapping[str, str]],
            timeout: tuple[float, float],
        ) -> requests.Response:
            seen["timeout"] = timeout
            return sentinel

    monkeypatch.setattr(http_session, "get_session", lambda: FakeSession())
    assert http_session.post("http://x", json={}) is sentinel
    assert seen["timeout"] == (
        settings.HTTP_CONNECT_TIMEOUT_SECONDS,
        settings.HTTP_READ_TIMEOUT_SECONDS,
    )


def test_session_retries_connect_errors_and_gateway_statuses() -> None:
    session = http_session._build_session()
    adapter = session.get_adapter("https://example.com")
    assert isinstance(adapter, HTTPAdapter)
    retry = adapter.max_retries
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
    assert fake.urls == ["http://p/x"]


# ---- run_with_retry / git clone ----------------------------------------


def completed(rc: int, stderr: str = "") -> subprocess.CompletedProcess[str]:
    return subprocess.CompletedProcess([], rc, "", stderr)


@dataclass
class RecordedCall:
    command: list[str]
    timeout: Optional[float]
    env: Optional[Mapping[str, str]] = None


class FakeRun:
    """Scripted replacement for subprocess.run as used by run_with_retry."""

    def __init__(self, *outcomes: subprocess.CompletedProcess[str] | Exception) -> None:
        self.outcomes = list(outcomes)
        self.calls: list[RecordedCall] = []

    def __call__(
        self,
        command: list[str],
        *,
        capture_output: bool,
        text: bool,
        timeout: Optional[float],
        env: Optional[Mapping[str, str]],
    ) -> subprocess.CompletedProcess[str]:
        self.calls.append(RecordedCall(command, timeout, env))
        outcome = self.outcomes.pop(0) if len(self.outcomes) > 1 else self.outcomes[0]
        if isinstance(outcome, Exception):
            raise outcome
        return outcome


class SubprocessRecorder:
    """Replacement for subprocess.run that only records calls."""

    def __init__(self) -> None:
        self.calls: list[RecordedCall] = []

    def __call__(
        self,
        command: list[str],
        *,
        check: bool = False,
        text: bool = False,
        timeout: Optional[float] = None,
    ) -> None:
        self.calls.append(RecordedCall(command, timeout))


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


def call_retry(
    clock: Clock,
    run: FakeRun,
    deadline: float = 100.0,
    cleanup: Optional[Callable[[], None]] = None,
) -> subprocess.CompletedProcess[str]:
    return process_utils.run_with_retry(
        ["git", "clone"],
        per_attempt_timeout=30,
        deadline=deadline,
        wait_seconds=2.0,
        is_transient=process_utils.is_transient_git_failure,
        cleanup=cleanup,
        run=run,
        sleep=clock.sleep,
        monotonic=clock.monotonic,
    )


def test_clone_nonzero_exit_is_not_swallowed_permanent() -> None:
    clock = Clock()
    run = FakeRun(completed(128, "fatal: repository 'x' not found"))
    with pytest.raises(process_utils.CommandFailed) as exc:
        call_retry(clock, run)
    assert not exc.value.transient
    assert len(run.calls) == 1  # permanent: no pointless waiting
    assert clock.slept == []


@pytest.mark.parametrize(
    "stderr",
    [
        "fatal: repository 'x' not found",
        "fatal: repository 'x' does not exist",
        "fatal: Remote branch v1 not found in upstream origin",
        "remote: Authentication failed for 'x'",
        "fatal: could not read Username for 'https://x'",
        "fatal: could not read Password for 'https://x'",
        "fatal: unable to access 'x': The requested URL returned error: 403",
        "fatal: unable to access 'x': The requested URL returned error: 404",
    ],
)
def test_known_git_messages_are_permanent(stderr: str) -> None:
    assert not process_utils.is_transient_git_failure(completed(128, stderr))


@pytest.mark.parametrize(
    "stderr",
    [
        "<html>502 Bad Gateway: upstream not found</html>",
        "fatal: not found",
        "fatal: Permission denied while proxying",
        "fatal: unable to access 'x': The requested URL returned error: 503",
        "",
    ],
)
def test_other_git_stderr_is_transient(stderr: str) -> None:
    assert process_utils.is_transient_git_failure(completed(128, stderr))


def test_clone_transient_failure_is_retried_until_success() -> None:
    clock = Clock()
    cleaned: list[int] = []
    run = FakeRun(
        completed(128, "fatal: unable to access: Could not resolve host"),
        subprocess.TimeoutExpired("git", 30),
        completed(0),
    )
    result = call_retry(clock, run, cleanup=lambda: cleaned.append(1))
    assert result.returncode == 0
    assert len(run.calls) == 3
    assert len(clock.slept) == 2 and len(cleaned) == 2


def test_clone_transient_failure_gives_up_at_deadline() -> None:
    clock = Clock()
    run = FakeRun(completed(128, "early EOF"))
    with pytest.raises(process_utils.CommandFailed) as exc:
        call_retry(clock, run, deadline=10.0)
    assert exc.value.transient
    assert clock.now <= 10.0 + 1e-9  # bounded


def test_every_attempt_has_timeout_capped_by_deadline() -> None:
    clock = Clock()
    run = FakeRun(subprocess.TimeoutExpired("git", 1), completed(0))
    call_retry(clock, run, deadline=35.0)
    assert run.calls[0].timeout == 30
    assert all(
        call.timeout is not None and 0 < call.timeout <= 30 for call in run.calls
    )


def test_curl_4xx_is_permanent_but_5xx_transient() -> None:
    f = process_utils.is_transient_curl_failure
    assert not f(completed(22, "curl: (22) The requested URL returned error: 404"))
    assert f(completed(22, "curl: (22) The requested URL returned error: 503"))
    assert f(completed(22, "curl: (22) The requested URL returned error: 429"))
    assert f(completed(28, "Operation timed out"))


def no_sleep(seconds: float) -> None:
    return None


def patch_process(
    monkeypatch: pytest.MonkeyPatch,
    run: FakeRun,
    sleep: Callable[[float], None] = no_sleep,
) -> None:
    real = process_utils.run_with_retry

    def with_fakes(
        command: Sequence[str],
        *,
        per_attempt_timeout: float,
        deadline: float,
        wait_seconds: float,
        is_transient: Callable[[subprocess.CompletedProcess[str]], bool],
        cleanup: Optional[Callable[[], None]] = None,
        env: Optional[Mapping[str, str]] = None,
    ) -> subprocess.CompletedProcess[str]:
        return real(
            command,
            per_attempt_timeout=per_attempt_timeout,
            deadline=deadline,
            wait_seconds=wait_seconds,
            is_transient=is_transient,
            cleanup=cleanup,
            env=env,
            run=run,
            sleep=sleep,
        )

    monkeypatch.setattr(resource_lookup, "run_with_retry", with_fakes)


def test_clone_git_repo_raises_on_failure_and_uses_arg_list(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    run = FakeRun(completed(128, "fatal: repository not found"))
    patch_process(monkeypatch, run)
    dest = str(tmp_path / "repo")
    with pytest.raises(HTTPException):
        resource_lookup.clone_git_repo(
            HttpUrl("https://example.com/o/r"),
            dest,
            branch="main",
        )
    command = run.calls[0].command
    assert command[:3] == ["git", "clone", "--depth=1"]
    assert "--branch" in command and command[-1] == dest
    timeout = run.calls[0].timeout
    assert timeout is not None and timeout <= settings.GIT_CLONE_TIMEOUT_SECONDS


def test_clone_git_repo_success(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    run = FakeRun(completed(0))
    patch_process(monkeypatch, run)
    resource_lookup.clone_git_repo(
        HttpUrl("https://example.com/o/r"),
        str(tmp_path / "repo"),
    )
    assert len(run.calls) == 1


def test_batch_clone_failure_is_raised_when_transient_exhausted(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    run = FakeRun(completed(128, "Could not resolve host"))
    # Tiny real ceiling and real (short) waits: retries happen, then it gives up.
    patch_process(monkeypatch, run, sleep=lambda s: time.sleep(0.02))
    monkeypatch.setattr(
        resource_lookup, "deadline_from_now", lambda _: time.monotonic() + 0.1
    )
    with pytest.raises(HTTPException):
        resource_lookup.batch_clone_git_repos(
            [(HttpUrl("https://example.com/o/r"), str(tmp_path / "r"))],
            asset_caching_enabled=False,
        )
    assert len(run.calls) > 1  # it waited and retried before giving up


def test_batch_clone_permanent_failure_does_not_block_other_repos(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    run = FakeRun(completed(128, "repository not found"), completed(0))
    patch_process(monkeypatch, run)
    resource_lookup.batch_clone_git_repos(
        [
            (HttpUrl("https://example.com/o/missing"), str(tmp_path / "a")),
            (HttpUrl("https://example.com/o/ok"), str(tmp_path / "b")),
        ],
        asset_caching_enabled=False,
    )
    assert len(run.calls) == 2
    assert all("||" not in " ".join(c.command) for c in run.calls)


# ---- timeouts on remaining subprocess/SMTP calls -----------------------


def test_converters_pass_timeout(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    recorder = SubprocessRecorder()
    monkeypatch.setattr(subprocess, "run", recorder)
    html = tmp_path / "a.html"
    html.write_text("<html></html>")
    document_generator.convert_html_to_epub(str(html), str(tmp_path / "a.epub"), "key")
    for use_prince in (False, True):
        document_generator.convert_html_to_pdf(
            str(html), str(tmp_path / "a.pdf"), "key", use_prince
        )
    assert len(recorder.calls) == 3
    assert all(c.timeout == settings.CONVERTER_TIMEOUT_SECONDS for c in recorder.calls)


def test_smtp_gets_timeout(monkeypatch: pytest.MonkeyPatch) -> None:
    seen: dict[str, object] = {}

    class FakeSMTP:
        def __init__(self, host: str, port: int, *, timeout: float) -> None:
            seen["timeout"] = timeout
            raise OSError("stop here")  # email errors are swallowed by design

    monkeypatch.setattr(smtplib, "SMTP", FakeSMTP)
    email_utils.send_email_with_attachment("a@example.com", [], "key")
    assert seen["timeout"] == settings.SMTP_TIMEOUT_SECONDS


def test_batch_download_uses_arg_lists_and_timeouts(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    run = FakeRun(completed(0))
    patch_process(monkeypatch, run)
    unzip = SubprocessRecorder()
    monkeypatch.setattr(subprocess, "run", unzip)
    resource_lookup.batch_download_repos(
        [(HttpUrl("https://example.com/o/r"), str(tmp_path / "r"))],
        resource_assets_dir=str(tmp_path),
    )
    curl = run.calls[0]
    assert curl.command[0] == "curl" and "--fail" in curl.command
    assert curl.timeout is not None
    assert curl.timeout <= settings.DOWNLOAD_TIMEOUT_SECONDS
    assert unzip.calls[0].timeout is not None
    assert unzip.calls[0].timeout <= settings.DOWNLOAD_TIMEOUT_SECONDS


def test_batch_download_total_is_bounded_by_batch_deadline(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    run = FakeRun(completed(0))
    patch_process(monkeypatch, run)
    monkeypatch.setattr(
        resource_lookup,
        "settings",
        settings.model_copy(update={"DOWNLOAD_BATCH_CEILING_SECONDS": 5}),
    )
    unzip = SubprocessRecorder()
    monkeypatch.setattr(subprocess, "run", unzip)
    resource_lookup.batch_download_repos(
        [
            (HttpUrl("https://example.com/o/a"), str(tmp_path / "a")),
            (HttpUrl("https://example.com/o/b"), str(tmp_path / "b")),
        ],
        resource_assets_dir=str(tmp_path),
    )
    assert run.calls and unzip.calls
    for call in run.calls + unzip.calls:
        assert call.timeout is not None and call.timeout <= 5


def test_batch_download_exhausted_batch_deadline_fails(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    run = FakeRun(completed(0))
    patch_process(monkeypatch, run)
    monkeypatch.setattr(
        resource_lookup,
        "settings",
        settings.model_copy(update={"DOWNLOAD_BATCH_CEILING_SECONDS": -1}),
    )
    with pytest.raises(HTTPException):
        resource_lookup.batch_download_repos(
            [(HttpUrl("https://example.com/o/a"), str(tmp_path / "a"))],
            resource_assets_dir=str(tmp_path),
        )


def test_timeouts_below_celery_soft_limit() -> None:
    soft_limit = 20 * 60
    assert settings.DOWNLOAD_BATCH_CEILING_SECONDS < soft_limit
    assert settings.GIT_CLONE_RETRY_CEILING_SECONDS < soft_limit
    assert settings.CONVERTER_TIMEOUT_SECONDS < soft_limit
    assert settings.DOWNLOAD_TIMEOUT_SECONDS < soft_limit
