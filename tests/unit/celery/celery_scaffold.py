"""
Test scaffold for exercising Celery tasks and configuration without
docker, Redis, or a live worker.

Two things live here:

* eager_celery_app: points the one DOC Celery app at in-memory broker
  and result backend URLs and makes tasks run eagerly in-process, then
  restores the app's configuration afterwards.
* Fakes for the outside world that tasks touch: the data API and USFM
  parser (both are HTTP POSTs through requests), subprocess (git,
  weasyprint, prince, ebook-convert, curl), and smtplib.

Task bodies cannot be called directly because they call
current_task.update_state, which needs a task context. Run them with
Task.apply(args=(...)) instead.
"""

import dataclasses
import json
import smtplib
import subprocess
from collections.abc import Iterator, Mapping, Sequence
from contextlib import contextmanager
from pathlib import Path
from types import TracebackType
from typing import Optional, Union

import requests
from celery import Celery
from doc.domain import worker

JsonValue = Union[
    str, int, float, bool, None, list["JsonValue"], dict[str, "JsonValue"]
]

EAGER_SETTINGS: dict[str, Union[str, bool]] = {
    "broker_url": "memory://",
    "result_backend": "cache+memory://",
    "task_always_eager": True,
    "task_eager_propagates": True,
}

USFM_PARSER_URL_MARKER = "/api/converter/convert"


@contextmanager
def eager_celery_config(app: Celery) -> Iterator[Celery]:
    """
    Configure app for in-process eager execution with in-memory broker
    and result backend, and restore its previous configuration on exit.
    """
    previous = {key: app.conf[key] for key in EAGER_SETTINGS}
    app.conf.update(EAGER_SETTINGS)
    try:
        yield app
    finally:
        app.conf.update(previous)


def load_task_modules(app: Celery = worker.app) -> None:
    """
    Import the modules named in the celeryconfig imports setting, which
    registers the tasks as the real worker does at startup.
    """
    app.loader.import_default_modules()


def document_request_json(
    overrides: Optional[Mapping[str, JsonValue]] = None,
) -> str:
    """
    Return a DocumentRequest as the JSON string that the API hands to
    the generate_document and generate_docx_document tasks.
    """
    request: dict[str, JsonValue] = {
        "email_address": None,
        "assembly_strategy_kind": "lbo",
        "assembly_layout_kind": "1c",
        "layout_for_print": False,
        "resource_requests": [
            {"lang_code": "es-419", "resource_type": "ulb", "book_code": "mat"}
        ],
        "generate_pdf": False,
        "generate_epub": False,
        "generate_docx": False,
        "chunk_size": "chapter",
        "limit_words": False,
        "show_tn_book_intro": False,
        "document_request_source": "ui",
    }
    if overrides:
        request.update(overrides)
    return json.dumps(request)


def data_api_payload(
    repos: Sequence[tuple[str, str, str]],
) -> dict[str, JsonValue]:
    """
    Build a GraphQL data API response body from (repo_url, resource_type,
    ietf_code) tuples.
    """
    git_repo: list[JsonValue] = [
        {
            "repo_url": repo_url,
            "content": {
                "resource_type": resource_type,
                "language": {
                    "english_name": ietf_code,
                    "ietf_code": ietf_code,
                    "national_name": ietf_code,
                    "direction": "ltr",
                },
            },
        }
        for repo_url, resource_type, ietf_code in repos
    ]
    return {"data": {"git_repo": git_repo}}


@dataclasses.dataclass
class FakeResponse:
    """The subset of requests.Response that DOC code uses."""

    payload: JsonValue
    status_code: int = 200

    def json(self) -> JsonValue:
        return self.payload

    def raise_for_status(self) -> None:
        if self.status_code >= 400:
            raise requests.HTTPError(f"{self.status_code} error")


@dataclasses.dataclass
class RecordedPost:
    url: str
    json: Optional[Mapping[str, JsonValue]]
    headers: Optional[Mapping[str, str]]
    timeout: object


@dataclasses.dataclass
class FakeHttp:
    """
    Stand-in for requests.post that answers both the data API and the
    USFM parser API, and records every call.

    Set data_api_error or usfm_parser_error to an exception to simulate
    an outage of that service.
    """

    data_api_response: FakeResponse = dataclasses.field(
        default_factory=lambda: FakeResponse(data_api_payload([]))
    )
    data_api_error: Optional[Exception] = None
    usfm_parser_html: str = "<p>fake usfm parser output</p>"
    usfm_parser_error: Optional[Exception] = None
    posts: list[RecordedPost] = dataclasses.field(default_factory=list)

    def post(
        self,
        url: str,
        data: object = None,
        json: Optional[Mapping[str, JsonValue]] = None,
        *,
        headers: Optional[Mapping[str, str]] = None,
        timeout: object = None,
        **kwargs: object,
    ) -> FakeResponse:
        self.posts.append(RecordedPost(url, json, headers, timeout))
        if USFM_PARSER_URL_MARKER in url:
            if self.usfm_parser_error is not None:
                raise self.usfm_parser_error
            output_file = json["OutputFile"] if json else None
            if isinstance(output_file, str):
                Path(output_file).write_text(self.usfm_parser_html)
            return FakeResponse(True)
        if self.data_api_error is not None:
            raise self.data_api_error
        return self.data_api_response

    def posts_to(self, url_fragment: str) -> list[RecordedPost]:
        return [post for post in self.posts if url_fragment in post.url]


Command = Union[str, Sequence[str]]


@dataclasses.dataclass
class FakeSubprocess:
    """
    Stand-in for the subprocess functions that DOC uses (run, call and
    check_call). Nothing is executed. Each command is recorded, and for
    an argument-list command that does not fail, the last argument is
    created as an empty-looking output file so that callers that go on
    to read the converted document find it.

    Set fail_with to an exception to make every command raise it, or
    exit_code to make call() return a nonzero exit code.
    """

    commands: list[Command] = dataclasses.field(default_factory=list)
    fail_with: Optional[BaseException] = None
    exit_code: int = 0
    output_bytes: bytes = b"fake converter output"

    def _record(self, args: Command) -> None:
        self.commands.append(args)
        if self.fail_with is not None:
            raise self.fail_with
        if (
            not isinstance(args, str)
            and len(args) > 1
            and not args[-1].startswith("-")
        ):
            output = Path(args[-1])
            if output.parent.is_dir():
                output.write_bytes(self.output_bytes)

    def run(
        self, args: Command, *, check: bool = False, **kwargs: object
    ) -> "subprocess.CompletedProcess[str]":
        self._record(args)
        if check and self.exit_code:
            raise subprocess.CalledProcessError(self.exit_code, args)
        return subprocess.CompletedProcess(args, self.exit_code, "", "")

    def call(self, args: Command, **kwargs: object) -> int:
        self._record(args)
        return self.exit_code

    def check_call(self, args: Command, **kwargs: object) -> int:
        self._record(args)
        if self.exit_code:
            raise subprocess.CalledProcessError(self.exit_code, args)
        return 0


@dataclasses.dataclass
class SentMail:
    sender: str
    recipients: Sequence[str]
    message: str


@dataclasses.dataclass
class FakeSmtp:
    """
    Stand-in for the smtplib.SMTP class. Calling it opens a session, as
    smtplib.SMTP(host, port) does, and the session can be used as a
    context manager. Sessions and the mail they were asked to send are
    recorded; nothing touches the network.

    Set login_error to an exception to simulate an SMTP outage.
    """

    login_error: Optional[Exception] = None
    connections: list[tuple[str, int]] = dataclasses.field(default_factory=list)
    logins: list[tuple[str, str]] = dataclasses.field(default_factory=list)
    sent: list[SentMail] = dataclasses.field(default_factory=list)

    def __call__(self, host: str = "", port: int = 0, **kwargs: object) -> "FakeSmtp":
        self.connections.append((host, port))
        return self

    def __enter__(self) -> "FakeSmtp":
        return self

    def __exit__(
        self,
        exc_type: Optional[type[BaseException]],
        exc: Optional[BaseException],
        tb: Optional[TracebackType],
    ) -> None:
        return None

    def ehlo(self) -> tuple[int, bytes]:
        return 250, b"ok"

    def starttls(self) -> tuple[int, bytes]:
        return 220, b"ok"

    def login(self, user: str, password: str) -> tuple[int, bytes]:
        if self.login_error is not None:
            raise self.login_error
        self.logins.append((user, password))
        return 235, b"ok"

    def sendmail(
        self, from_addr: str, to_addrs: Sequence[str], msg: str
    ) -> dict[str, tuple[int, bytes]]:
        self.sent.append(SentMail(from_addr, to_addrs, msg))
        return {}

    def close(self) -> None:
        return None


def smtp_error() -> smtplib.SMTPException:
    return smtplib.SMTPServerDisconnected("fake SMTP outage")

