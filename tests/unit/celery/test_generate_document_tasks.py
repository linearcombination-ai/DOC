"""
Run the real generate_document and generate_docx_document tasks through
Task.apply() with the scaffold's fakes standing in for subprocess and
smtplib.
"""

import subprocess
from pathlib import Path

import pytest
from celery import Celery
from doc.domain import document_generator

from tests.unit.celery.celery_scaffold import (
    FakeSmtp,
    FakeSubprocess,
    document_request_json,
)

EMAIL = "reader@example.org"


def test_docx_task_with_nothing_to_generate_returns_request_key(
    eager_celery_app: Celery, document_output_dir: Path
) -> None:
    result = document_generator.generate_docx_document.apply(
        args=(document_request_json(),)
    )
    assert result.successful()
    assert result.state == "SUCCESS"
    key = result.get()
    assert key.startswith("es-419_ulb_mat_")


def test_identical_requests_get_the_same_key_and_different_requests_do_not(
    eager_celery_app: Celery, document_output_dir: Path
) -> None:
    def key_for(request: str) -> str:
        result = document_generator.generate_docx_document.apply(args=(request,))
        key: str = result.get()
        return key

    base = document_request_json()
    assert key_for(base) == key_for(document_request_json())
    assert key_for(base) != key_for(document_request_json({"layout_for_print": True}))
    assert key_for(base) != key_for(
        document_request_json(
            {
                "resource_requests": [
                    {"lang_code": "es-419", "resource_type": "ulb", "book_code": "mrk"}
                ]
            }
        )
    )


def test_pdf_is_converted_with_subprocess_and_emailed(
    eager_celery_app: Celery,
    document_output_dir: Path,
    fake_subprocess: FakeSubprocess,
    fake_smtp: FakeSmtp,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr(
        document_generator, "should_send_email", lambda email_address: True
    )
    request = document_request_json({"generate_pdf": True, "email_address": EMAIL})
    # Pre-build the HTML so that the task goes straight to PDF conversion
    # instead of locating and parsing resources.
    pdf_key = document_generator.initialize_document_request_and_key(request)[1]
    (document_output_dir / f"{pdf_key}.html").write_text("<html></html>")

    result = document_generator.generate_document.apply(args=(request,))

    assert result.get() == pdf_key
    pdf_path = document_output_dir / f"{pdf_key}.pdf"
    assert fake_subprocess.commands == [
        ["weasyprint", str(document_output_dir / f"{pdf_key}.html"), str(pdf_path)]
    ]
    assert len(fake_smtp.sent) == 1
    assert list(fake_smtp.sent[0].recipients) == [EMAIL]
    assert pdf_key in fake_smtp.sent[0].message
    assert len(fake_smtp.logins) == 1


def test_up_to_date_outputs_skip_conversion_and_email(
    eager_celery_app: Celery,
    document_output_dir: Path,
    fake_subprocess: FakeSubprocess,
    fake_smtp: FakeSmtp,
) -> None:
    request = document_request_json({"generate_pdf": True})
    key = document_generator.initialize_document_request_and_key(request)[1]
    (document_output_dir / f"{key}.html").write_text("<html></html>")
    (document_output_dir / f"{key}.pdf").write_bytes(b"%PDF")

    result = document_generator.generate_document.apply(args=(request,))

    assert result.get() == key
    assert fake_subprocess.commands == []
    assert fake_smtp.connections == []


def test_converter_failure_propagates_from_eager_task(
    eager_celery_app: Celery,
    document_output_dir: Path,
    fake_subprocess: FakeSubprocess,
) -> None:
    fake_subprocess.fail_with = subprocess.CalledProcessError(1, ["weasyprint"])
    request = document_request_json({"generate_pdf": True})
    key = document_generator.initialize_document_request_and_key(request)[1]
    (document_output_dir / f"{key}.html").write_text("<html></html>")

    with pytest.raises(subprocess.CalledProcessError):
        document_generator.generate_document.apply(args=(request,))


def test_converter_failure_is_recorded_as_task_failure_when_not_propagated(
    eager_celery_app: Celery,
    document_output_dir: Path,
    fake_subprocess: FakeSubprocess,
) -> None:
    fake_subprocess.fail_with = subprocess.CalledProcessError(1, ["weasyprint"])
    request = document_request_json({"generate_pdf": True})
    key = document_generator.initialize_document_request_and_key(request)[1]
    (document_output_dir / f"{key}.html").write_text("<html></html>")

    result = document_generator.generate_document.apply(
        args=(request,), throw=False
    )

    assert result.failed()
    assert result.state == "FAILURE"
    assert isinstance(result.result, subprocess.CalledProcessError)
