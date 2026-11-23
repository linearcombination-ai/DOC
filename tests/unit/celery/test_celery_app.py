"""Tests of the DOC Celery app itself: registration and the scaffold."""

import pytest
from celery import Celery
from doc.domain import celeryconfig, worker

from tests.unit.celery.celery_scaffold import (
    EAGER_SETTINGS,
    eager_celery_config,
    load_task_modules,
)

EXPECTED_TASK_NAMES = {
    "doc.domain.document_generator.generate_document",
    "doc.domain.document_generator.generate_docx_document",
    "doc.domain.resource_lookup.resource_types",
    "doc.domain.resource_lookup.book_codes_for_lang",
    "doc.domain.resource_lookup.book_codes_for_lang_from_usfm_only",
    "passages.domain.document_generator.generate_passages_docx_document",
    "passages.domain.document_generator.stet_exhaustive_verse_list",
    "stet.domain.document_generator.generate_stet_docx_document",
}


def test_all_tasks_are_registered_by_the_configured_imports() -> None:
    load_task_modules(worker.app)
    # The celery and pytest-celery packages register their own tasks
    # (pytest-celery is a dev dependency); only DOC's tasks are of interest.
    registered = {
        name
        for name in worker.app.tasks
        if not name.startswith(("celery.", "pytest_celery."))
    }
    assert registered == EXPECTED_TASK_NAMES


def test_every_configured_import_is_importable_and_defines_a_task() -> None:
    # A task in a module missing from imports is silently unregistered
    # on the worker, so each module listed must be real and must be
    # where some expected task lives.
    load_task_modules(worker.app)
    for module_name in celeryconfig.imports:
        assert any(
            name.startswith(module_name + ".") for name in EXPECTED_TASK_NAMES
        ), module_name


def test_tasks_use_json_serialization() -> None:
    # Task arguments and results are strings and primitives.
    assert worker.app.conf.task_serializer == "json"
    assert worker.app.conf.result_serializer == "json"
    assert list(worker.app.conf.accept_content) == ["json"]


def test_eager_fixture_configures_in_memory_app(eager_celery_app: Celery) -> None:
    assert eager_celery_app is worker.app
    for key, value in EAGER_SETTINGS.items():
        assert eager_celery_app.conf[key] == value
    assert eager_celery_app.conf.broker_url == "memory://"
    assert eager_celery_app.conf.result_backend == "cache+memory://"


def test_eager_config_is_restored_afterwards() -> None:
    before = {key: worker.app.conf[key] for key in EAGER_SETTINGS}
    with eager_celery_config(worker.app):
        assert worker.app.conf.task_always_eager is True
    assert {key: worker.app.conf[key] for key in EAGER_SETTINGS} == before


def test_eager_config_is_restored_after_an_error() -> None:
    before = {key: worker.app.conf[key] for key in EAGER_SETTINGS}
    with pytest.raises(RuntimeError):
        with eager_celery_config(worker.app):
            raise RuntimeError("boom")
    assert {key: worker.app.conf[key] for key in EAGER_SETTINGS} == before
