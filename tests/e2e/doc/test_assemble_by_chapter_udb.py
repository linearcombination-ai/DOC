import pytest
from doc.config import get_settings
from doc.domain import model
from doc.entrypoints.app import app
from fastapi.testclient import TestClient

from tests.shared.utils import check_finished_document_with_verses_success


logger = get_settings().logger(__name__)

pytest.skip("Skipping this module", allow_module_level=True)


def test_en_ulb_col_en_tn_col_en_tq_col_en_tw_col_tl_ulb_col_tl_tn_col_tl_tq_col_tl_tw_col_tl_udb_col_by_chapter_1c() -> (
    None
):
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_CHAPTER,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN,
                "layout_for_print": False,
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
                        "lang_code": "tl",
                        "resource_type": "ulb",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "tl",
                        "resource_type": "tn",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "tl",
                        "resource_type": "tq",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "tl",
                        "resource_type": "tw",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "tl",
                        "resource_type": "udb",
                        "book_code": "col",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response)


def test_en_ulb_col_en_tn_col_en_tq_col_en_tw_col_tl_ulb_col_tl_tn_col_tl_tq_col_tl_tw_col_tl_udb_col_by_chapter_1c_c() -> (
    None
):
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_CHAPTER,
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
                        "lang_code": "tl",
                        "resource_type": "ulb",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "tl",
                        "resource_type": "tn",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "tl",
                        "resource_type": "tq",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "tl",
                        "resource_type": "tw",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "tl",
                        "resource_type": "udb",
                        "book_code": "col",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response)


def test_fr_ulb_rev_fr_tw_rev_fr_udb_rev_ndh_x_chindali_reg_mat_ndh_x_chindali_tn_mat_ndh_x_chindali_tq_mat_ndh_x_chindali_tw_mat_ndh_x_chindali_udb_mat_by_chapter_1c() -> (
    None
):
    """
    Show that the succeeding resource request's, fr-f10-rev,
    content is rendered and the failing resource requests are reported
    on the cover page of the PDF.
    """
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_CHAPTER,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN,
                "layout_for_print": False,
                "generate_pdf": False,
                "generate_epub": False,
                "generate_docx": False,
                "resource_requests": [
                    {
                        "lang_code": "fr",
                        "resource_type": "ulb",
                        "book_code": "rev",
                    },
                    {
                        "lang_code": "fr",
                        "resource_type": "tw",
                        "book_code": "rev",
                    },
                    {
                        "lang_code": "fr",
                        "resource_type": "f10",
                        "book_code": "rev",
                    },
                    {
                        "lang_code": "ndh-x-chindali",
                        "resource_type": "reg",
                        "book_code": "mat",
                    },
                    {
                        "lang_code": "ndh-x-chindali",
                        "resource_type": "tn",
                        "book_code": "mat",
                    },
                    {
                        "lang_code": "ndh-x-chindali",
                        "resource_type": "tq",
                        "book_code": "mat",
                    },
                    {
                        "lang_code": "ndh-x-chindali",
                        "resource_type": "tw",
                        "book_code": "mat",
                    },
                    {
                        "lang_code": "ndh-x-chindali",
                        "resource_type": "udb",
                        "book_code": "mat",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response)


def test_fr_ulb_rev_fr_tw_rev_fr_udb_rev_ndh_x_chindali_reg_mat_ndh_x_chindali_tn_mat_ndh_x_chindali_tq_mat_ndh_x_chindali_tw_mat_ndh_x_chindali_udb_mat_by_chapter_1c_c() -> (
    None
):
    """
    Show that the succeeding resource request's, fr-f10-rev,
    content is rendered and the failing resource requests are reported
    on the cover page of the PDF.
    """
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_CHAPTER,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN_COMPACT,
                "layout_for_print": True,
                "generate_pdf": False,
                "generate_epub": False,
                "generate_docx": False,
                "resource_requests": [
                    {
                        "lang_code": "fr",
                        "resource_type": "ulb",
                        "book_code": "rev",
                    },
                    {
                        "lang_code": "fr",
                        "resource_type": "tw",
                        "book_code": "rev",
                    },
                    {
                        "lang_code": "fr",
                        "resource_type": "f10",
                        "book_code": "rev",
                    },
                    {
                        "lang_code": "ndh-x-chindali",
                        "resource_type": "reg",
                        "book_code": "mat",
                    },
                    {
                        "lang_code": "ndh-x-chindali",
                        "resource_type": "tn",
                        "book_code": "mat",
                    },
                    {
                        "lang_code": "ndh-x-chindali",
                        "resource_type": "tq",
                        "book_code": "mat",
                    },
                    {
                        "lang_code": "ndh-x-chindali",
                        "resource_type": "tw",
                        "book_code": "mat",
                    },
                    {
                        "lang_code": "ndh-x-chindali",
                        "resource_type": "udb",
                        "book_code": "mat",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response)


def test_ndh_x_chindali_reg_mat_ndh_x_chindali_tn_mat_ndh_x_chindali_tq_mat_ndh_x_chindali_tw_mat_ndh_x_chindali_udb_mat_by_chapter() -> (
    None
):
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_CHAPTER,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN,
                "layout_for_print": False,
                "generate_pdf": False,
                "generate_epub": False,
                "generate_docx": False,
                "resource_requests": [
                    {
                        "lang_code": "ndh-x-chindali",
                        "resource_type": "reg",
                        "book_code": "mat",
                    },
                    {
                        "lang_code": "ndh-x-chindali",
                        "resource_type": "tn",
                        "book_code": "mat",
                    },
                    {
                        "lang_code": "ndh-x-chindali",
                        "resource_type": "tq",
                        "book_code": "mat",
                    },
                    {
                        "lang_code": "ndh-x-chindali",
                        "resource_type": "tw",
                        "book_code": "mat",
                    },
                    {
                        "lang_code": "ndh-x-chindali",
                        "resource_type": "udb",
                        "book_code": "mat",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response)
