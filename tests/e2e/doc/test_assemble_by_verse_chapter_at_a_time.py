"""
Tests for assembly strategy book-then-language
"""

import pytest
from doc.config import get_settings
from doc.domain import model
from doc.entrypoints.app import app
from fastapi.testclient import TestClient

from tests.shared.utils import check_finished_document_with_verses_success

logger = get_settings().logger(__name__)


# en tn is not currently available from data API
@pytest.mark.skip
def test_en_ulb_col_en_tn_col_en_tq_col_en_tw_col_fr_f10_col_fr_tn_col_fr_tq_col_fr_tw_col_by_verse_chapter_at_a_time_1c_c() -> (
    None
):
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_VERSE,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN_COMPACT,
                "layout_for_print": True,
                "generate_pdf": False,
                "generate_epub": False,
                "generate_docx": False,
                "resource_requests": [
                    {
                        "lang_code": "en",
                        "resource_type": "ulb",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "en",
                        "resource_type": "tn",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "en",
                        "resource_type": "tq",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "en",
                        "resource_type": "tw",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "fr",
                        "resource_type": "f10",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "fr",
                        "resource_type": "tn",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "fr",
                        "resource_type": "tq",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "fr",
                        "resource_type": "tw",
                        "book_code": "col",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response)


# en tn is not currently available from data API
@pytest.mark.skip
def test_en_ulb_col_en_tn_col_en_tq_col_en_tw_col_pt_br_ulb_col_pt_br_tn_col_pt_br_tq_col_pt_br_tw_col_by_chapter_1c() -> (
    None
):
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_VERSE,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN,
                "layout_for_print": False,
                "generate_pdf": True,
                "generate_epub": False,
                "generate_docx": False,
                "resource_requests": [
                    {
                        "lang_code": "en",
                        "resource_type": "ulb",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "en",
                        "resource_type": "tn",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "en",
                        "resource_type": "tq",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "en",
                        "resource_type": "tw",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "pt-br",
                        "resource_type": "ulb",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "pt-br",
                        "resource_type": "tn",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "pt-br",
                        "resource_type": "tq",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "pt-br",
                        "resource_type": "tw",
                        "book_code": "col",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response)
