import pytest
from doc.config import get_settings
from doc.domain import model
from doc.entrypoints.app import app
from fastapi.testclient import TestClient

from tests.shared.utils import (
    check_finished_document_with_verses_success,
)

logger = get_settings().logger(__name__)


@pytest.mark.skip
def test_en_ulb_col_by_verse_with_no_email_1c() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                # "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_VERSE_BOOK_AT_A_TIME,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN,
                "layout_for_print": False,
                "generate_pdf": True,
                "generate_epub": False,
                "generate_docx": False,
                "use_prince": True,
                "resource_requests": [
                    {
                        "lang_code": "en",
                        "resource_type": "ulb",
                        "book_code": "col",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response, suffix="pdf")
