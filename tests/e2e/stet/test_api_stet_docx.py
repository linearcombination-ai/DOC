import os
import pytest
from fastapi.testclient import TestClient
from tests.shared.utils import (
    check_result,
    document_contains_substring,
    AcceptedSuffixes,
)
from doc.entrypoints.app import app
from doc.config import get_settings
from docx import Document  # type: ignore


@pytest.mark.stet
@pytest.mark.docx
def test_en_es_419_stet_docx() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/stet/documents_docx",
            json={
                "lang0_code": "en",
                "lang1_code": "es-419",
                "email_address": get_settings().TO_EMAIL_ADDRESS,
            },
        )
        check_result(response, suffix="docx")


@pytest.mark.stet
@pytest.mark.docx
def test_en_abu_stet_docx() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/stet/documents_docx",
            json={
                "lang0_code": "en",
                "lang1_code": "abu",
                "email_address": get_settings().TO_EMAIL_ADDRESS,
            },
        )
        check_result(response, suffix="docx")


@pytest.mark.stet
@pytest.mark.docx
@pytest.mark.skip
def test_en_ln_stet_docx() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/stet/documents_docx",
            json={
                "lang0_code": "en",
                "lang1_code": "ln",
                "email_address": get_settings().TO_EMAIL_ADDRESS,
            },
        )
        check_result(response, suffix="docx")


@pytest.mark.stet
@pytest.mark.docx
@pytest.mark.skip
def test_en_ln_stet_docx_contents() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/stet/documents_docx",
            json={
                "lang0_code": "en",
                "lang1_code": "ln",
                "email_address": get_settings().TO_EMAIL_ADDRESS,
            },
        )
        # Check that all of Luke 1:49 is present in result
        substring = (
            "For the Mighty One has done great things for me,and his name is holy."
        )
        suffix: AcceptedSuffixes = "docx"
        finished_document_request_key = check_result(response, suffix=suffix)
        finished_document_path = os.path.join(
            get_settings().DOCUMENT_OUTPUT_DIR,
            "{}.{}".format(finished_document_request_key, suffix),
        )
        doc = Document(finished_document_path)
        assert document_contains_substring(doc, substring)
        # Luke 1:48
        substring = "For he has looked at the low condition of his female servant.For see, from now on all generations will call me blessed."
        assert document_contains_substring(doc, substring)
