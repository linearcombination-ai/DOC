"""
Fixtures for the Celery unit tests. The implementation lives in
celery_scaffold so that it can be imported by name from other test
modules as well.
"""

import smtplib
import subprocess
from collections.abc import Iterator
from pathlib import Path

import pytest
import requests
from celery import Celery
from doc.domain import document_generator, resource_lookup, worker

from tests.unit.celery.celery_scaffold import (
    FakeHttp,
    FakeSmtp,
    FakeSubprocess,
    eager_celery_config,
    load_task_modules,
)


@pytest.fixture
def eager_celery_app() -> Iterator[Celery]:
    """
    The DOC Celery app configured for in-process eager execution with
    in-memory broker and result backend, with its tasks registered.
    Configuration is restored afterwards.
    """
    load_task_modules(worker.app)
    with eager_celery_config(worker.app) as app:
        yield app


@pytest.fixture
def fake_http(monkeypatch: pytest.MonkeyPatch) -> Iterator[FakeHttp]:
    """
    Fake data API and USFM parser, installed as requests.post. The data
    API result caches are emptied before and after so that fakes never
    leak between tests.
    """
    fake = FakeHttp()
    monkeypatch.setattr(requests, "post", fake.post)
    resource_lookup.fetch_source_data_cache.clear()
    resource_lookup.fetch_non_primary_source_data_cache.clear()
    yield fake
    resource_lookup.fetch_source_data_cache.clear()
    resource_lookup.fetch_non_primary_source_data_cache.clear()


@pytest.fixture
def fake_subprocess(monkeypatch: pytest.MonkeyPatch) -> FakeSubprocess:
    """Fake subprocess.run, call and check_call that run nothing."""
    fake = FakeSubprocess()
    monkeypatch.setattr(subprocess, "run", fake.run)
    monkeypatch.setattr(subprocess, "call", fake.call)
    monkeypatch.setattr(subprocess, "check_call", fake.check_call)
    return fake


@pytest.fixture
def fake_smtp(monkeypatch: pytest.MonkeyPatch) -> FakeSmtp:
    """Fake smtplib.SMTP that records mail instead of sending it."""
    fake = FakeSmtp()
    monkeypatch.setattr(smtplib, "SMTP", fake)
    return fake


@pytest.fixture
def document_output_dir(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> Path:
    """
    Redirect where generate_document and generate_docx_document read
    and write their output files to a temporary directory, and treat a
    file as needing an update only if it does not exist yet.
    """

    monkeypatch.setattr(
        document_generator,
        "html_filepath",
        lambda key: str(tmp_path / f"{key}.html"),
    )
    monkeypatch.setattr(
        document_generator,
        "pdf_filepath",
        lambda key: str(tmp_path / f"{key}.pdf"),
    )
    monkeypatch.setattr(
        document_generator,
        "epub_filepath",
        lambda key: str(tmp_path / f"{key}.epub"),
    )
    monkeypatch.setattr(
        document_generator,
        "docx_filepath",
        lambda key: str(tmp_path / f"{key}.docx"),
    )
    monkeypatch.setattr(
        document_generator,
        "file_needs_update",
        lambda path: not Path(path).exists(),
    )
    return tmp_path
