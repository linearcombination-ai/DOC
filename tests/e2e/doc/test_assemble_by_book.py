import pytest
from doc.config import get_settings
from doc.entrypoints.app import app
from fastapi.testclient import TestClient
from tests.shared.utils import (
    check_finished_document_with_verses_success,
    check_finished_document_without_verses_success,
)
from doc.domain import model


def test_en_ulb_tit_en_tn_tit_by_book_1c() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN,
                "layout_for_print": False,
                "generate_pdf": False,
                "generate_epub": False,
                "generate_docx": False,
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
        check_finished_document_with_verses_success(response)


def test_en_ulb_tit_en_tn_tit_by_book_1c_c() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN_COMPACT,
                "layout_for_print": True,
                "generate_pdf": False,
                "generate_epub": False,
                "generate_docx": False,
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
        check_finished_document_with_verses_success(response)


def test_sw_ulb_col_sw_tn_col_by_book_1c() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN,
                "layout_for_print": False,
                "generate_pdf": False,
                "generate_epub": False,
                "generate_docx": False,
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
        check_finished_document_with_verses_success(response)


def test_sw_ulb_col_sw_tn_col_by_book_1c_c() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN_COMPACT,
                "layout_for_print": True,
                "generate_pdf": False,
                "generate_epub": False,
                "generate_docx": False,
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
        check_finished_document_with_verses_success(response)


def test_sw_ulb_col_sw_tn_col_sw_ulb_tit_sw_tn_tit_by_book_1c() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN,
                "layout_for_print": False,
                "generate_pdf": False,
                "generate_epub": False,
                "generate_docx": False,
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
        check_finished_document_with_verses_success(response)


def test_sw_ulb_col_sw_tn_col_sw_ulb_tit_sw_tn_tit_by_book_1c_c() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN_COMPACT,
                "layout_for_print": True,
                "generate_pdf": False,
                "generate_epub": False,
                "generate_docx": False,
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
        check_finished_document_with_verses_success(response)


def test_en_ulb_col_en_tn_col_sw_ulb_col_sw_tn_col_sw_ulb_tit_sw_tn_tit_by_book_1c() -> (
    None
):
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
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
        check_finished_document_with_verses_success(response)


def test_en_ulb_col_en_tn_col_sw_ulb_col_sw_tn_col_sw_ulb_tit_sw_tn_tit_by_book_1c_c() -> (
    None
):
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN,
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
        check_finished_document_with_verses_success(response)


def test_en_ulb_col_en_tn_col_en_tq_col_sw_ulb_col_sw_tn_col_sw_tq_col_sw_ulb_tit_sw_tn_tit_sw_tq_tit_by_book_1c() -> (
    None
):
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
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
        check_finished_document_with_verses_success(response)


def test_en_ulb_col_en_tn_col_en_tq_col_sw_ulb_col_sw_tn_col_sw_tq_col_sw_ulb_tit_sw_tn_tit_sw_tq_tit_by_book_1c_c() -> (
    None
):
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
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
        check_finished_document_with_verses_success(response)


def test_en_ulb_col_en_tq_col_sw_ulb_col_sw_tq_col_sw_ulb_tit_sw_tq_tit_by_book_1c() -> (
    None
):
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
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
        check_finished_document_with_verses_success(response)


def test_en_ulb_col_en_tq_col_sw_ulb_col_sw_tq_col_sw_ulb_tit_sw_tq_tit_by_book_1c_c() -> (
    None
):
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
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
        check_finished_document_with_verses_success(response)


def test_en_tn_col_en_tq_col_en_tw_col_sw_tn_col_sw_tq_col_sw_tw_col_sw_tn_tit_sw_tq_tit_sw_tw_tit_by_book_1c() -> (
    None
):
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN,
                "layout_for_print": False,
                "generate_pdf": False,
                "generate_epub": False,
                "generate_docx": False,
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
        check_finished_document_without_verses_success(response)


def test_en_tn_col_en_tq_col_en_tw_col_sw_tn_col_sw_tq_col_sw_tw_col_sw_tn_tit_sw_tq_tit_sw_tw_tit_by_book_1c_c() -> (
    None
):
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN_COMPACT,
                "layout_for_print": True,
                "generate_pdf": False,
                "generate_epub": False,
                "generate_docx": False,
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
        check_finished_document_without_verses_success(response)


def test_en_tn_col_en_tw_col_sw_tn_col_sw_tw_col_sw_tn_tit_sw_tw_tit_by_book_1c() -> (
    None
):
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN,
                "layout_for_print": False,
                "generate_pdf": False,
                "generate_epub": False,
                "generate_docx": False,
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
        check_finished_document_without_verses_success(response)


@pytest.mark.skip
def test_en_tn_col_en_tw_col_sw_tn_col_sw_tw_col_sw_tn_tit_sw_tw_tit_by_book_1c_c() -> (
    None
):
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN_COMPACT,
                "layout_for_print": True,
                "generate_pdf": False,
                "generate_epub": False,
                "generate_docx": False,
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
        check_finished_document_without_verses_success(response)


def test_en_tq_col_en_tw_col_sw_tq_col_sw_tw_col_sw_tq_tit_sw_tw_tit_by_book_1c() -> (
    None
):
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN,
                "layout_for_print": False,
                "generate_pdf": False,
                "generate_epub": False,
                "generate_docx": False,
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
        check_finished_document_without_verses_success(response)


def test_en_tq_col_en_tw_col_sw_tq_col_sw_tw_col_sw_tq_tit_sw_tw_tit_by_book_1c_c() -> (
    None
):
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN_COMPACT,
                "layout_for_print": True,
                "generate_pdf": False,
                "generate_epub": False,
                "generate_docx": False,
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
        check_finished_document_without_verses_success(response)


def test_en_tw_col_sw_tw_col_sw_tw_tit_by_book_1c() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN,
                "layout_for_print": False,
                "generate_pdf": False,
                "generate_epub": False,
                "generate_docx": False,
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
        check_finished_document_without_verses_success(response)


def test_en_tn_col_en_tq_col_sw_tn_col_sw_tq_col_sw_tn_tit_sw_tq_tit_by_book_1c() -> (
    None
):
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN,
                "layout_for_print": False,
                "generate_pdf": False,
                "generate_epub": False,
                "generate_docx": False,
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
        check_finished_document_without_verses_success(response)


def test_en_tn_col_en_tq_col_sw_tn_col_sw_tq_col_sw_tn_tit_sw_tq_tit_by_book_1c_c() -> (
    None
):
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN_COMPACT,
                "layout_for_print": True,
                "generate_pdf": False,
                "generate_epub": False,
                "generate_docx": False,
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
        check_finished_document_without_verses_success(response)


def test_en_tq_col_sw_tq_col_sw_tq_tit_by_book_1c() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN,
                "layout_for_print": False,
                "generate_pdf": False,
                "generate_epub": False,
                "generate_docx": False,
                "resource_requests": [
                    {
                        "lang_code": "en",
                        "resource_type": "tq",
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
        check_finished_document_without_verses_success(response)


def test_en_tq_col_sw_tq_col_sw_tq_tit_by_book_1c_c() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN_COMPACT,
                "layout_for_print": False,
                "generate_pdf": False,
                "generate_epub": False,
                "generate_docx": False,
                "resource_requests": [
                    {
                        "lang_code": "en",
                        "resource_type": "tq",
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
        check_finished_document_without_verses_success(response)


def test_en_tn_col_sw_tn_col_sw_tn_tit_by_book_1c() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN,
                "layout_for_print": False,
                "generate_pdf": False,
                "generate_epub": False,
                "generate_docx": False,
                "resource_requests": [
                    {
                        "lang_code": "en",
                        "resource_type": "tn",
                        "book_code": "col",
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
        check_finished_document_without_verses_success(response)


def test_en_tn_col_sw_tn_col_sw_tn_tit_by_book_1c_c() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN_COMPACT,
                "layout_for_print": True,
                "generate_pdf": False,
                "generate_epub": False,
                "generate_docx": False,
                "resource_requests": [
                    {
                        "lang_code": "en",
                        "resource_type": "tn",
                        "book_code": "col",
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
        check_finished_document_without_verses_success(response)


def test_en_ulb_col_sw_ulb_col_sw_ulb_tit_by_book_1c() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
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
        check_finished_document_with_verses_success(response)


def test_en_ulb_col_sw_ulb_col_sw_ulb_tit_by_book_1c_c() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
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
        check_finished_document_with_verses_success(response)


def test_tl_tq_gen_tl_ulb_gen_by_book_1c() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN,
                "layout_for_print": False,
                "generate_pdf": False,
                "generate_epub": False,
                "generate_docx": False,
                "resource_requests": [
                    {
                        "lang_code": "tl",
                        "resource_type": "tq",
                        "book_code": "gen",
                    },
                    {
                        "lang_code": "tl",
                        "resource_type": "ulb",
                        "book_code": "gen",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response)


def test_fr_ulb_rev_fr_tn_rev_fr_tq_rev_fr_tw_rev_fr_f10_rev_by_book_1c_c() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
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
        check_finished_document_with_verses_success(response)


def test_fr_ulb_rev_fr_tn_rev_fr_tq_rev_fr_tw_rev_fr_f10_rev_by_book_1c() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
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
        check_finished_document_with_verses_success(response)


def test_fr_ulb_rev_fr_tq_rev_fr_tw_rev_fr_f10_rev_by_book_1c() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
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
        check_finished_document_with_verses_success(response)


def test_fr_ulb_rev_fr_tq_rev_fr_tw_rev_fr_f10_rev_by_book_1c_c() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
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
        check_finished_document_with_verses_success(response)


@pytest.mark.skip
def test_fr_ulb_rev_fr_tw_rev_fr_f10_rev_by_book_1c() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
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
                ],
            },
        )
        check_finished_document_with_verses_success(response)


@pytest.mark.skip
def test_fr_ulb_rev_fr_tw_rev_fr_f10_rev_by_book_1c_c() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
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
                ],
            },
        )
        check_finished_document_with_verses_success(response)


def test_en_ulb_col_en_tn_col_en_tq_col_en_tw_col_es_419_ulb_col_es_419_tn_col_es_419_tq_col_es_419_tw_col_by_book_1c() -> (
    None
):
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
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
        check_finished_document_with_verses_success(response)


def test_en_ulb_col_en_tn_col_en_tq_col_en_tw_col_es_419_ulb_col_es_419_tn_col_es_419_tq_col_es_419_tw_col_by_book_1c_c() -> (
    None
):
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
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
        check_finished_document_with_verses_success(response)


# NOTE This test succeeds in that it throws an expected exception
# (oxymoron I know). The problem is that celery can't pickle it because
# the exception's type differs from what celery's pickling subsystem was
# expecting. This can be and was diagnosed by consulting the celery flower
# dashboard for the task generated by this test.
@pytest.mark.skip
def test_kbt_reg_2co_ajg_reg_2co_pmm_reg_mrk_by_book_1c() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN,
                "layout_for_print": False,
                "generate_pdf": False,
                "generate_epub": False,
                "generate_docx": False,
                "resource_requests": [
                    {
                        "lang_code": "kbt",
                        "resource_type": "reg",
                        "book_code": "2co",
                    },
                    {
                        "lang_code": "ajg",
                        "resource_type": "reg",
                        "book_code": "2co",
                    },
                    {
                        "lang_code": "pmm",
                        "resource_type": "reg",
                        "book_code": "mrk",
                    },
                ],
            },
        )
        # kbt has a malformed asset URL in translations.json so we
        # expect to fail obtaining it through git cloning.
        with pytest.raises(Exception):
            check_finished_document_with_verses_success(response)


# NOTE This test succeeds in that it throws an expected exception
# (oxymoron I know). The problem is that celery can't pickle it because
# the exception's type differs from what celery's pickling subsystem was
# expecting. This can be and was diagnosed by consulting the celery flower
# dashboard for the task generated by this test.
@pytest.mark.skip
def test_kbt_reg_2co_ajg_reg_2co_pmm_reg_mrk_by_book_1c_c() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN_COMPACT,
                "layout_for_print": True,
                "generate_pdf": False,
                "generate_epub": False,
                "generate_docx": False,
                "resource_requests": [
                    {
                        "lang_code": "kbt",
                        "resource_type": "reg",
                        "book_code": "2co",
                    },
                    {
                        "lang_code": "ajg",
                        "resource_type": "reg",
                        "book_code": "2co",
                    },
                    {
                        "lang_code": "pmm",
                        "resource_type": "reg",
                        "book_code": "mrk",
                    },
                ],
            },
        )
        # kbt has a malformed asset URL in translations.json so we
        # expect to fail obtaining it through git cloning.
        with pytest.raises(Exception):
            check_finished_document_with_verses_success(response)


# NOTE This test succeeds in that it throws an expected exception
# (oxymoron I know). The problem is that celery can't pickle it because
# the exception's type differs from what celery's pickling subsystem was
# expecting. This can be and was diagnosed by consulting the celery flower
# dashboard for the task generated by this test.
@pytest.mark.skip
def test_kbt_reg_2co_ajg_x_adjtalagbe_reg_2co_pmm_reg_mrk_by_book_1c() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN,
                "layout_for_print": False,
                "generate_pdf": False,
                "generate_epub": False,
                "generate_docx": False,
                "resource_requests": [
                    {
                        "lang_code": "kbt",
                        "resource_type": "reg",
                        "book_code": "2co",
                    },
                    {
                        "lang_code": "ajg-x-adjtalagbe",
                        "resource_type": "reg",
                        "book_code": "2co",
                    },
                    {
                        "lang_code": "pmm",
                        "resource_type": "reg",
                        "book_code": "mrk",
                    },
                ],
            },
        )
        # kbt has a malformed asset URL in translations.json so we
        # expect to fail obtaining it through git cloning.
        with pytest.raises(Exception):
            check_finished_document_with_verses_success(response)


# NOTE This test succeeds in that it throws an expected exception
# (oxymoron I know). The problem is that celery can't pickle it because
# the exception's type differs from what celery's pickling subsystem was
# expecting. This can be and was diagnosed by consulting the celery flower
# dashboard for the task generated by this test.
@pytest.mark.skip
def test_kbt_reg_2co_ajg_x_adjtalagbe_reg_2co_pmm_reg_mrk_by_book_1c_c() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN_COMPACT,
                "layout_for_print": True,
                "generate_pdf": False,
                "generate_epub": False,
                "generate_docx": False,
                "resource_requests": [
                    {
                        "lang_code": "kbt",
                        "resource_type": "reg",
                        "book_code": "2co",
                    },
                    {
                        "lang_code": "ajg-x-adjtalagbe",
                        "resource_type": "reg",
                        "book_code": "2co",
                    },
                    {
                        "lang_code": "pmm",
                        "resource_type": "reg",
                        "book_code": "mrk",
                    },
                ],
            },
        )
        # kbt has a malformed asset URL in translations.json so we
        # expect to fail obtaining it through git cloning.
        with pytest.raises(Exception):
            check_finished_document_with_verses_success(response)


def test_id_ayt_tit_id_tn_tit_by_book_1c() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN,
                "layout_for_print": False,
                "generate_pdf": False,
                "generate_epub": False,
                "generate_docx": False,
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
                ],
            },
        )
        check_finished_document_with_verses_success(response)


def test_en_ulb_mat_en_bc_mat_by_book_1c() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN,
                "layout_for_print": False,
                "generate_pdf": False,
                "generate_epub": False,
                "generate_docx": False,
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
        check_finished_document_with_verses_success(response)
