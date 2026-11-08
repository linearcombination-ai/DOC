import pytest
from doc.config import get_settings
from doc.entrypoints.app import app
from fastapi.testclient import TestClient
from tests.shared.utils import (
    check_finished_document_with_verses_success,
    check_finished_document_without_verses_success,
)
from doc.domain import model

pytest.skip("Skipping this module", allow_module_level=True)


def test_gu_ulb_mrk_gu_tn_mrk_gu_tq_mrk_gu_tw_mrk_gu_udb_mrk_by_book_1c() -> None:
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
                        "lang_code": "gu",
                        "resource_type": "ulb",
                        "book_code": "mrk",
                    },
                    {
                        "lang_code": "gu",
                        "resource_type": "tn",
                        "book_code": "mrk",
                    },
                    {
                        "lang_code": "gu",
                        "resource_type": "tq",
                        "book_code": "mrk",
                    },
                    {
                        "lang_code": "gu",
                        "resource_type": "tw",
                        "book_code": "mrk",
                    },
                    {
                        "lang_code": "gu",
                        "resource_type": "udb",
                        "book_code": "mrk",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response)


def test_gu_ulb_mrk_gu_tn_mrk_gu_tq_mrk_gu_tw_mrk_gu_udb_mrk_by_book_1c_c() -> None:
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
                        "lang_code": "gu",
                        "resource_type": "ulb",
                        "book_code": "mrk",
                    },
                    {
                        "lang_code": "gu",
                        "resource_type": "tn",
                        "book_code": "mrk",
                    },
                    {
                        "lang_code": "gu",
                        "resource_type": "tq",
                        "book_code": "mrk",
                    },
                    {
                        "lang_code": "gu",
                        "resource_type": "tw",
                        "book_code": "mrk",
                    },
                    {
                        "lang_code": "gu",
                        "resource_type": "udb",
                        "book_code": "mrk",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response)


def test_mr_ulb_mrk_mr_tn_mrk_mr_tq_mrk_mr_tw_mrk_mr_udb_mrk_by_book_1c() -> None:
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
                        "lang_code": "mr",
                        "resource_type": "ulb",
                        "book_code": "mrk",
                    },
                    {
                        "lang_code": "mr",
                        "resource_type": "tn",
                        "book_code": "mrk",
                    },
                    {
                        "lang_code": "mr",
                        "resource_type": "tq",
                        "book_code": "mrk",
                    },
                    {
                        "lang_code": "mr",
                        "resource_type": "tw",
                        "book_code": "mrk",
                    },
                    {
                        "lang_code": "mr",
                        "resource_type": "udb",
                        "book_code": "mrk",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response)


def test_mr_ulb_mrk_mr_tn_mrk_mr_tq_mrk_mr_tw_mrk_mr_udb_mrk_by_book_1c_c() -> None:
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
                        "lang_code": "mr",
                        "resource_type": "ulb",
                        "book_code": "mrk",
                    },
                    {
                        "lang_code": "mr",
                        "resource_type": "tn",
                        "book_code": "mrk",
                    },
                    {
                        "lang_code": "mr",
                        "resource_type": "tq",
                        "book_code": "mrk",
                    },
                    {
                        "lang_code": "mr",
                        "resource_type": "tw",
                        "book_code": "mrk",
                    },
                    {
                        "lang_code": "mr",
                        "resource_type": "udb",
                        "book_code": "mrk",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response)


def test_mr_ulb_mrk_mr_tn_mrk_mr_tq_mrk_mr_udb_mrk_by_book_1c() -> None:
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
                        "lang_code": "mr",
                        "resource_type": "ulb",
                        "book_code": "mrk",
                    },
                    {
                        "lang_code": "mr",
                        "resource_type": "tn",
                        "book_code": "mrk",
                    },
                    {
                        "lang_code": "mr",
                        "resource_type": "tq",
                        "book_code": "mrk",
                    },
                    {
                        "lang_code": "mr",
                        "resource_type": "udb",
                        "book_code": "mrk",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response)


def test_mr_ulb_mrk_mr_tn_mrk_mr_tq_mrk_mr_udb_mrk_by_book_1c_c() -> None:
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
                        "lang_code": "mr",
                        "resource_type": "ulb",
                        "book_code": "mrk",
                    },
                    {
                        "lang_code": "mr",
                        "resource_type": "tn",
                        "book_code": "mrk",
                    },
                    {
                        "lang_code": "mr",
                        "resource_type": "tq",
                        "book_code": "mrk",
                    },
                    {
                        "lang_code": "mr",
                        "resource_type": "udb",
                        "book_code": "mrk",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response)


def test_mr_ulb_mrk_mr_tn_mrk_mr_tw_mrk_mr_udb_mrk_by_book_1c() -> None:
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
                        "lang_code": "mr",
                        "resource_type": "ulb",
                        "book_code": "mrk",
                    },
                    {
                        "lang_code": "mr",
                        "resource_type": "tn",
                        "book_code": "mrk",
                    },
                    {
                        "lang_code": "mr",
                        "resource_type": "tw",
                        "book_code": "mrk",
                    },
                    {
                        "lang_code": "mr",
                        "resource_type": "udb",
                        "book_code": "mrk",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response)


def test_mr_ulb_mrk_mr_tn_mrk_mr_tw_mrk_mr_udb_mrk_by_book_1c_c() -> None:
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
                        "lang_code": "mr",
                        "resource_type": "ulb",
                        "book_code": "mrk",
                    },
                    {
                        "lang_code": "mr",
                        "resource_type": "tn",
                        "book_code": "mrk",
                    },
                    {
                        "lang_code": "mr",
                        "resource_type": "tw",
                        "book_code": "mrk",
                    },
                    {
                        "lang_code": "mr",
                        "resource_type": "udb",
                        "book_code": "mrk",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response)


def test_mr_ulb_mrk_mr_tn_mrk_mr_udb_mrk_by_book_1c() -> None:
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
                        "lang_code": "mr",
                        "resource_type": "ulb",
                        "book_code": "mrk",
                    },
                    {
                        "lang_code": "mr",
                        "resource_type": "tn",
                        "book_code": "mrk",
                    },
                    {
                        "lang_code": "mr",
                        "resource_type": "udb",
                        "book_code": "mrk",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response)


def test_mr_ulb_mrk_mr_tn_mrk_mr_udb_mrk_by_book_1c_c() -> None:
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
                        "lang_code": "mr",
                        "resource_type": "ulb",
                        "book_code": "mrk",
                    },
                    {
                        "lang_code": "mr",
                        "resource_type": "tn",
                        "book_code": "mrk",
                    },
                    {
                        "lang_code": "mr",
                        "resource_type": "udb",
                        "book_code": "mrk",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response)


def test_mr_ulb_mrk_mr_tq_mrk_mr_udb_mrk_by_book_1c() -> None:
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
                        "lang_code": "mr",
                        "resource_type": "ulb",
                        "book_code": "mrk",
                    },
                    {
                        "lang_code": "mr",
                        "resource_type": "tq",
                        "book_code": "mrk",
                    },
                    {
                        "lang_code": "mr",
                        "resource_type": "udb",
                        "book_code": "mrk",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response)


def test_mr_ulb_mrk_mr_tq_mrk_mr_udb_mrk_by_book_1c_c() -> None:
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
                        "lang_code": "mr",
                        "resource_type": "ulb",
                        "book_code": "mrk",
                    },
                    {
                        "lang_code": "mr",
                        "resource_type": "tq",
                        "book_code": "mrk",
                    },
                    {
                        "lang_code": "mr",
                        "resource_type": "udb",
                        "book_code": "mrk",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response)


def test_tl_ulb_gen_tl_udb_gen_by_book_1c() -> None:
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
                        "resource_type": "ulb",
                        "book_code": "gen",
                    },
                    {
                        "lang_code": "tl",
                        "resource_type": "udb",
                        "book_code": "gen",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response)


def test_tl_ulb_gen_tl_udb_gen_by_book_1c_c() -> None:
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
                        "lang_code": "tl",
                        "resource_type": "ulb",
                        "book_code": "gen",
                    },
                    {
                        "lang_code": "tl",
                        "resource_type": "udb",
                        "book_code": "gen",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response)


def test_gu_tn_mat_gu_tq_mat_gu_tw_mat_gu_udb_mat_by_book_1c() -> None:
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
                        "lang_code": "gu",
                        "resource_type": "tn",
                        "book_code": "mat",
                    },
                    {
                        "lang_code": "gu",
                        "resource_type": "tq",
                        "book_code": "mat",
                    },
                    {
                        "lang_code": "gu",
                        "resource_type": "tw",
                        "book_code": "mat",
                    },
                    {
                        "lang_code": "gu",
                        "resource_type": "udb",
                        "book_code": "mat",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response)


def test_gu_tn_mat_gu_tq_mat_gu_tw_mat_gu_udb_mat_by_book_1c_c() -> None:
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
                        "lang_code": "gu",
                        "resource_type": "tn",
                        "book_code": "mat",
                    },
                    {
                        "lang_code": "gu",
                        "resource_type": "tq",
                        "book_code": "mat",
                    },
                    {
                        "lang_code": "gu",
                        "resource_type": "tw",
                        "book_code": "mat",
                    },
                    {
                        "lang_code": "gu",
                        "resource_type": "udb",
                        "book_code": "mat",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response)


def test_gu_tn_mat_gu_tq_mat_gu_udb_mat_by_book_1c() -> None:
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
                        "lang_code": "gu",
                        "resource_type": "tn",
                        "book_code": "mat",
                    },
                    {
                        "lang_code": "gu",
                        "resource_type": "tq",
                        "book_code": "mat",
                    },
                    {
                        "lang_code": "gu",
                        "resource_type": "udb",
                        "book_code": "mat",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response)


def test_gu_tn_mat_gu_tq_mat_gu_udb_mat_by_book_1c_c() -> None:
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
                        "lang_code": "gu",
                        "resource_type": "tn",
                        "book_code": "mat",
                    },
                    {
                        "lang_code": "gu",
                        "resource_type": "tq",
                        "book_code": "mat",
                    },
                    {
                        "lang_code": "gu",
                        "resource_type": "udb",
                        "book_code": "mat",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response)


def test_tl_tn_gen_tl_tw_gen_tl_udb_gen_by_book_1c() -> None:
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
                        "resource_type": "tn",
                        "book_code": "gen",
                    },
                    {
                        "lang_code": "tl",
                        "resource_type": "tw",
                        "book_code": "gen",
                    },
                    {
                        "lang_code": "tl",
                        "resource_type": "udb",
                        "book_code": "gen",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response)


def test_tl_tn_gen_tl_tw_gen_tl_udb_gen_by_book_1c_c() -> None:
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
                        "lang_code": "tl",
                        "resource_type": "tn",
                        "book_code": "gen",
                    },
                    {
                        "lang_code": "tl",
                        "resource_type": "tw",
                        "book_code": "gen",
                    },
                    {
                        "lang_code": "tl",
                        "resource_type": "udb",
                        "book_code": "gen",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response)


def test_tl_tq_gen_tl_udb_gen_by_book_1c_c() -> None:
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
                        "lang_code": "tl",
                        "resource_type": "tq",
                        "book_code": "gen",
                    },
                    {
                        "lang_code": "tl",
                        "resource_type": "udb",
                        "book_code": "gen",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response)


def test_tl_tw_gen_tl_udb_gen_by_book_1c() -> None:
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
                        "resource_type": "tw",
                        "book_code": "gen",
                    },
                    {
                        "lang_code": "tl",
                        "resource_type": "udb",
                        "book_code": "gen",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response)


def test_tl_tw_gen_tl_udb_gen_by_book_1c_c() -> None:
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
                        "lang_code": "tl",
                        "resource_type": "tw",
                        "book_code": "gen",
                    },
                    {
                        "lang_code": "tl",
                        "resource_type": "udb",
                        "book_code": "gen",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response)


def test_tl_udb_gen_by_book_1c() -> None:
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
                        "resource_type": "udb",
                        "book_code": "gen",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response)


def test_tl_udb_gen_by_book_1c_c() -> None:
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
                        "lang_code": "tl",
                        "resource_type": "udb",
                        "book_code": "gen",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response)
