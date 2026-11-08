import pytest
from doc.config import get_settings
from doc.entrypoints.app import app
from fastapi.testclient import TestClient
from tests.shared.utils import check_result
from doc.domain import model


@pytest.mark.docx
def test_en_ulb_tit_en_tn_tit_by_book_1c_docx() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents_docx",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN,
                "layout_for_print": False,
                "generate_pdf": False,
                "generate_epub": False,
                "generate_docx": True,
                "resource_requests": [
                    {
                        "lang_code": "en",
                        "resource_type": "ulb",
                        "book_code": "tit",
                    },
                    {
                        "lang_code": "en",
                        "resource_type": "tn",
                        "book_code": "tit",
                    },
                ],
            },
        )
        check_result(response, suffix="docx")


@pytest.mark.docx
def test_sw_ulb_col_sw_tn_col_by_book_1c_docx() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents_docx",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN,
                "layout_for_print": False,
                "generate_pdf": False,
                "generate_epub": False,
                "generate_docx": True,
                "resource_requests": [
                    {
                        "lang_code": "sw",
                        "resource_type": "ulb",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "sw",
                        "resource_type": "tn",
                        "book_code": "col",
                    },
                ],
            },
        )
        check_result(response, suffix="docx")


@pytest.mark.docx
def test_sw_ulb_col_sw_tn_col_sw_ulb_tit_sw_tn_tit_by_book_1c_docx() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents_docx",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN,
                "layout_for_print": False,
                "generate_pdf": False,
                "generate_epub": False,
                "generate_docx": True,
                "resource_requests": [
                    {
                        "lang_code": "sw",
                        "resource_type": "ulb",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "sw",
                        "resource_type": "tn",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "sw",
                        "resource_type": "ulb",
                        "book_code": "tit",
                    },
                    {
                        "lang_code": "sw",
                        "resource_type": "tn",
                        "book_code": "tit",
                    },
                ],
            },
        )
        check_result(response, suffix="docx")


@pytest.mark.docx
def test_en_ulb_col_en_tn_col_sw_ulb_col_sw_tn_col_sw_ulb_tit_sw_tn_tit_by_book_1c_docx() -> (
    None
):
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents_docx",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN,
                "layout_for_print": False,
                "generate_pdf": False,
                "generate_epub": False,
                "generate_docx": True,
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
                        "lang_code": "sw",
                        "resource_type": "ulb",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "sw",
                        "resource_type": "tn",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "sw",
                        "resource_type": "ulb",
                        "book_code": "tit",
                    },
                    {
                        "lang_code": "sw",
                        "resource_type": "tn",
                        "book_code": "tit",
                    },
                ],
            },
        )
        check_result(response, suffix="docx")


@pytest.mark.docx
def test_en_ulb_col_en_tn_col_en_tq_col_sw_ulb_col_sw_tn_col_sw_tq_col_sw_ulb_tit_sw_tn_tit_sw_tq_tit_by_book_1c_docx() -> (
    None
):
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents_docx",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN,
                "layout_for_print": False,
                "generate_pdf": False,
                "generate_epub": False,
                "generate_docx": True,
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
                        "lang_code": "sw",
                        "resource_type": "ulb",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "sw",
                        "resource_type": "tn",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "sw",
                        "resource_type": "tq",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "sw",
                        "resource_type": "ulb",
                        "book_code": "tit",
                    },
                    {
                        "lang_code": "sw",
                        "resource_type": "tn",
                        "book_code": "tit",
                    },
                    {
                        "lang_code": "sw",
                        "resource_type": "tq",
                        "book_code": "tit",
                    },
                ],
            },
        )
        check_result(response, suffix="docx")


@pytest.mark.docx
def test_en_ulb_col_en_tq_col_sw_ulb_col_sw_tq_col_sw_ulb_tit_sw_tq_tit_by_book_1c_docx() -> (
    None
):
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents_docx",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN,
                "layout_for_print": False,
                "generate_pdf": False,
                "generate_epub": False,
                "generate_docx": True,
                "resource_requests": [
                    {
                        "lang_code": "en",
                        "resource_type": "ulb",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "en",
                        "resource_type": "tq",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "sw",
                        "resource_type": "ulb",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "sw",
                        "resource_type": "tq",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "sw",
                        "resource_type": "ulb",
                        "book_code": "tit",
                    },
                    {
                        "lang_code": "sw",
                        "resource_type": "tq",
                        "book_code": "tit",
                    },
                ],
            },
        )
        check_result(response, suffix="docx")


@pytest.mark.docx
def test_en_tn_col_en_tq_col_en_tw_col_en_tq_tit_en_tw_tit_sw_tn_col_sw_tq_col_sw_tw_col_sw_tn_tit_sw_tq_tit_sw_tw_tit_by_book_1c_docx() -> (
    None
):
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents_docx",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN,
                "layout_for_print": False,
                "generate_pdf": False,
                "generate_epub": False,
                "generate_docx": True,
                "resource_requests": [
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
                        "lang_code": "en",
                        "resource_type": "tn",
                        "book_code": "tit",
                    },
                    {
                        "lang_code": "en",
                        "resource_type": "tq",
                        "book_code": "tit",
                    },
                    {
                        "lang_code": "en",
                        "resource_type": "tw",
                        "book_code": "tit",
                    },
                    {
                        "lang_code": "sw",
                        "resource_type": "tn",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "sw",
                        "resource_type": "tq",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "sw",
                        "resource_type": "tw",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "sw",
                        "resource_type": "tn",
                        "book_code": "tit",
                    },
                    {
                        "lang_code": "sw",
                        "resource_type": "tq",
                        "book_code": "tit",
                    },
                    {
                        "lang_code": "sw",
                        "resource_type": "tw",
                        "book_code": "tit",
                    },
                ],
            },
        )
        check_result(response, suffix="docx")


@pytest.mark.docx
def test_en_tn_col_en_tw_col_sw_tn_col_sw_tw_col_sw_tn_tit_sw_tw_tit_by_book_1c_docx() -> (
    None
):
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents_docx",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN,
                "layout_for_print": False,
                "generate_pdf": False,
                "generate_epub": False,
                "generate_docx": True,
                "resource_requests": [
                    {
                        "lang_code": "en",
                        "resource_type": "tn",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "en",
                        "resource_type": "tw",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "sw",
                        "resource_type": "tn",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "sw",
                        "resource_type": "tw",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "sw",
                        "resource_type": "tn",
                        "book_code": "tit",
                    },
                    {
                        "lang_code": "sw",
                        "resource_type": "tw",
                        "book_code": "tit",
                    },
                ],
            },
        )
        check_result(response, suffix="docx")


@pytest.mark.docx
def test_en_tq_col_en_tw_col_sw_tq_col_sw_tw_col_sw_tq_tit_sw_tw_tit_by_book_1c_docx() -> (
    None
):
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents_docx",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN,
                "layout_for_print": False,
                "generate_pdf": False,
                "generate_epub": False,
                "generate_docx": True,
                "resource_requests": [
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
                        "lang_code": "sw",
                        "resource_type": "tq",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "sw",
                        "resource_type": "tw",
                        "book_code": "col",
                    },
                ],
            },
        )
        check_result(response, suffix="docx")


@pytest.mark.docx
def test_en_tw_col_sw_tw_col_by_book_1c_docx() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents_docx",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN,
                "layout_for_print": False,
                "generate_pdf": False,
                "generate_epub": False,
                "generate_docx": True,
                "resource_requests": [
                    {
                        "lang_code": "en",
                        "resource_type": "tw",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "sw",
                        "resource_type": "tw",
                        "book_code": "col",
                    },
                ],
            },
        )
        check_result(response, suffix="docx")


@pytest.mark.docx
def test_en_tn_col_en_tq_col_sw_tn_col_sw_tq_col_by_book_1c_docx() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents_docx",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN,
                "layout_for_print": False,
                "generate_pdf": False,
                "generate_epub": False,
                "generate_docx": True,
                "resource_requests": [
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
                        "lang_code": "sw",
                        "resource_type": "tn",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "sw",
                        "resource_type": "tq",
                        "book_code": "col",
                    },
                ],
            },
        )
        check_result(response, suffix="docx")


@pytest.mark.docx
def test_en_tn_col_sw_tn_col_sw_tn_tit_by_book_1c_docx() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents_docx",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN,
                "layout_for_print": False,
                "generate_pdf": False,
                "generate_epub": False,
                "generate_docx": True,
                "resource_requests": [
                    {
                        "lang_code": "en",
                        "resource_type": "tn",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "en",
                        "resource_type": "tn",
                        "book_code": "tit",
                    },
                    {
                        "lang_code": "sw",
                        "resource_type": "tn",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "sw",
                        "resource_type": "tn",
                        "book_code": "tit",
                    },
                ],
            },
        )
        check_result(response, suffix="docx")


@pytest.mark.docx
def test_en_ulb_col_sw_ulb_col_sw_ulb_tit_by_book_1c_docx() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents_docx",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN,
                "layout_for_print": False,
                "generate_pdf": False,
                "generate_epub": False,
                "generate_docx": True,
                "resource_requests": [
                    {
                        "lang_code": "en",
                        "resource_type": "ulb",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "sw",
                        "resource_type": "ulb",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "sw",
                        "resource_type": "ulb",
                        "book_code": "tit",
                    },
                ],
            },
        )
        check_result(response, suffix="docx")


@pytest.mark.docx
def test_fr_ulb_rev_fr_tn_rev_fr_tq_rev_fr_tw_rev_fr_f10_rev_by_book_1c_docx() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents_docx",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN,
                "layout_for_print": False,
                "generate_pdf": False,
                "generate_epub": False,
                "generate_docx": True,
                "resource_requests": [
                    {
                        "lang_code": "fr",
                        "resource_type": "ulb",
                        "book_code": "rev",
                    },
                    {
                        "lang_code": "fr",
                        "resource_type": "tn",
                        "book_code": "rev",
                    },
                    {
                        "lang_code": "fr",
                        "resource_type": "tq",
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
                ],
            },
        )
        check_result(response, suffix="docx")


@pytest.mark.docx
def test_fr_ulb_rev_fr_tn_rev_fr_tq_rev_fr_tw_rev_fr_f10_rev_by_book_1c_docx() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents_docx",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN,
                "layout_for_print": False,
                "generate_pdf": False,
                "generate_epub": False,
                "generate_docx": True,
                "resource_requests": [
                    {
                        "lang_code": "fr",
                        "resource_type": "ulb",
                        "book_code": "rev",
                    },
                    {
                        "lang_code": "fr",
                        "resource_type": "tn",
                        "book_code": "rev",
                    },
                    {
                        "lang_code": "fr",
                        "resource_type": "tq",
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
                ],
            },
        )
        check_result(response, suffix="docx")


@pytest.mark.docx
def test_fr_ulb_rev_fr_tq_rev_fr_tw_rev_fr_f10_rev_by_book_1c_docx() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents_docx",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN,
                "layout_for_print": False,
                "generate_pdf": False,
                "generate_epub": False,
                "generate_docx": True,
                "resource_requests": [
                    {
                        "lang_code": "fr",
                        "resource_type": "ulb",
                        "book_code": "rev",
                    },
                    {
                        "lang_code": "fr",
                        "resource_type": "tq",
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
                ],
            },
        )
        check_result(response, suffix="docx")


@pytest.mark.docx
def test_fr_ulb_rev_fr_tw_rev_fr_f10_rev_by_book_1c_docx() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents_docx",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN,
                "layout_for_print": False,
                "generate_pdf": False,
                "generate_epub": False,
                "generate_docx": True,
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
                ],
            },
        )
        check_result(response, suffix="docx")


@pytest.mark.docx
def test_en_ulb_col_en_tn_col_en_tq_col_en_tw_col_es_419_ulb_col_es_419_tn_col_es_419_tq_col_es_419_tw_col_by_book_1c_docx() -> (
    None
):
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents_docx",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN,
                "layout_for_print": False,
                "generate_pdf": False,
                "generate_epub": False,
                "generate_docx": True,
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
                        "lang_code": "es-419",
                        "resource_type": "ulb",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "es-419",
                        "resource_type": "tn",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "es-419",
                        "resource_type": "tq",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "es-419",
                        "resource_type": "tw",
                        "book_code": "col",
                    },
                ],
            },
        )
        check_result(response, suffix="docx")


@pytest.mark.docx
def test_id_ayt_tit_id_tn_tit_id_tq_tit_id_tw_tit_by_book_1c_docx() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents_docx",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN,
                "layout_for_print": False,
                "generate_pdf": False,
                "generate_epub": False,
                "generate_docx": True,
                "resource_requests": [
                    {
                        "lang_code": "id",
                        "resource_type": "ayt",
                        "book_code": "tit",
                    },
                    {
                        "lang_code": "id",
                        "resource_type": "tn",
                        "book_code": "tit",
                    },
                    {
                        "lang_code": "id",
                        "resource_type": "tq",
                        "book_code": "tit",
                    },
                    {
                        "lang_code": "id",
                        "resource_type": "tw",
                        "book_code": "tit",
                    },
                ],
            },
        )
        check_result(response, suffix="docx")


@pytest.mark.docx
def test_en_ulb_mat_en_bc_mat_by_book_1c_docx() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents_docx",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN,
                "layout_for_print": False,
                "generate_pdf": False,
                "generate_epub": False,
                "generate_docx": True,
                "resource_requests": [
                    {
                        "lang_code": "en",
                        "resource_type": "ulb",
                        "book_code": "mat",
                    },
                    {
                        "lang_code": "en",
                        "resource_type": "bc",
                        "book_code": "mat",
                    },
                ],
            },
        )
        check_result(response, suffix="docx")
