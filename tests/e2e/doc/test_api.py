"""This module provides tests for the application's FastAPI API."""

import os
import re

import pytest
from doc.config import get_settings
from doc.domain import model
from doc.entrypoints.app import app
from fastapi.testclient import TestClient

from tests.shared.utils import (
    check_finished_document_with_body_success,
    check_finished_document_with_verses_success,
    check_finished_document_without_verses_success,
    check_result,
)

logger = get_settings().logger(__name__)


@pytest.mark.docx
def test_en_ulb_col_en_tn_col_language_book_order_with_no_email_1c_docx() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents_docx",
            json={
                # "email_address": get_settings().TO_EMAIL_ADDRESS,
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
                ],
            },
        )
        check_result(response, suffix="docx")


def test_fr_ulb_col_fr_tn_col_language_book_order_with_no_email_1c_c() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                # "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN_COMPACT,
                "layout_for_print": True,
                "generate_pdf": True,
                "generate_epub": False,
                "generate_docx": False,
                "use_prince": True,
                "resource_requests": [
                    {
                        "lang_code": "fr",
                        "resource_type": "ulb",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "fr",
                        "resource_type": "tn",
                        "book_code": "col",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response, suffix="pdf")


def test_fr_ulb_wa_col_en_fr_wa_col_fr_tq_wa_col_language_book_order_1c() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN,
                "layout_for_print": False,
                "generate_pdf": True,
                "generate_epub": False,
                "generate_docx": False,
                "use_prince": True,
                "resource_requests": [
                    {
                        "lang_code": "fr",
                        "resource_type": "ulb",
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
                ],
            },
        )
        check_finished_document_with_verses_success(response, suffix="pdf")


def test_fr_ulb_col_fr_tn_col_fr_tq_col_language_book_order_1c_c() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN_COMPACT,
                "layout_for_print": True,
                "generate_pdf": True,
                "generate_epub": False,
                "generate_docx": False,
                "use_prince": True,
                "resource_requests": [
                    {
                        "lang_code": "fr",
                        "resource_type": "ulb",
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
                ],
            },
        )
        check_finished_document_with_verses_success(response, suffix="pdf")


@pytest.mark.skip
def test_en_ulb_tn_jud_language_book_order_1c() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN,
                "layout_for_print": False,
                "generate_pdf": True,
                "generate_epub": False,
                "generate_docx": False,
                "resource_requests": [
                    {
                        "lang_code": "en",
                        "resource_type": "ulb",
                        "book_code": "jud",
                    },
                    {
                        "lang_code": "en",
                        "resource_type": "tn",
                        "book_code": "jud",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response, suffix="pdf")


@pytest.mark.skip
def test_en_ulb_tn_jud_language_book_order_1c_c() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN_COMPACT,
                "layout_for_print": True,
                "generate_pdf": True,
                "generate_epub": False,
                "generate_docx": False,
                "resource_requests": [
                    {
                        "lang_code": "en",
                        "resource_type": "ulb",
                        "book_code": "jud",
                    },
                    {
                        "lang_code": "en",
                        "resource_type": "tn",
                        "book_code": "jud",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response, suffix="pdf")


def test_en_ulb_tn_condensed_jud_language_book_order_1c() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN,
                "layout_for_print": False,
                "generate_pdf": True,
                "generate_epub": False,
                "generate_docx": False,
                "resource_requests": [
                    {
                        "lang_code": "en",
                        "resource_type": "ulb",
                        "book_code": "jud",
                    },
                    {
                        "lang_code": "en",
                        "resource_type": "tn-condensed",
                        "book_code": "jud",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response, suffix="pdf")


def test_pt_br_ulb_gen_pt_br_tn_gen_language_book_order_1c() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN,
                "layout_for_print": False,
                "generate_pdf": True,
                "generate_epub": False,
                "generate_docx": False,
                "use_prince": True,
                "resource_requests": [
                    {
                        "lang_code": "pt-br",
                        "resource_type": "ulb",
                        "book_code": "gen",
                    },
                    {
                        "lang_code": "pt-br",
                        "resource_type": "tn",
                        "book_code": "gen",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response, suffix="pdf")


def test_pt_br_ulb_tn_language_book_order_1c_c() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN_COMPACT,
                "layout_for_print": True,
                "generate_pdf": True,
                "generate_epub": False,
                "generate_docx": False,
                "use_prince": True,
                "resource_requests": [
                    {
                        "lang_code": "pt-br",
                        "resource_type": "ulb",
                        "book_code": "gen",
                    },
                    {
                        "lang_code": "pt-br",
                        "resource_type": "tn",
                        "book_code": "gen",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response, suffix="pdf")


def test_pt_br_ulb_tn_en_ulb_tn_luk_language_book_order_1c() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN,
                "layout_for_print": False,
                "generate_pdf": True,
                "generate_epub": False,
                "generate_docx": False,
                "use_prince": True,
                "resource_requests": [
                    {
                        "lang_code": "pt-br",
                        "resource_type": "ulb",
                        "book_code": "luk",
                    },
                    {
                        "lang_code": "pt-br",
                        "resource_type": "tn",
                        "book_code": "luk",
                    },
                    {
                        "lang_code": "en",
                        "resource_type": "ulb",
                        "book_code": "luk",
                    },
                    {
                        "lang_code": "en",
                        "resource_type": "tn",
                        "book_code": "luk",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response, suffix="pdf")


def test_pt_br_ulb_tn_en_ulb_tn_luk_language_book_order_1c_c() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN_COMPACT,
                "layout_for_print": True,
                "generate_pdf": True,
                "generate_epub": False,
                "generate_docx": False,
                "use_prince": True,
                "resource_requests": [
                    {
                        "lang_code": "pt-br",
                        "resource_type": "ulb",
                        "book_code": "luk",
                    },
                    {
                        "lang_code": "pt-br",
                        "resource_type": "tn",
                        "book_code": "luk",
                    },
                    {
                        "lang_code": "en",
                        "resource_type": "ulb",
                        "book_code": "luk",
                    },
                    {
                        "lang_code": "en",
                        "resource_type": "tn",
                        "book_code": "luk",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response, suffix="pdf")


def test_pt_br_ulb_tn_luk_en_ulb_tn_luk_sw_ulb_tn_col_language_book_order_1c() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN,
                "layout_for_print": False,
                "generate_pdf": True,
                "generate_epub": False,
                "generate_docx": False,
                "use_prince": True,
                "resource_requests": [
                    {
                        "lang_code": "pt-br",
                        "resource_type": "ulb",
                        "book_code": "luk",
                    },
                    {
                        "lang_code": "pt-br",
                        "resource_type": "tn",
                        "book_code": "luk",
                    },
                    {
                        "lang_code": "en",
                        "resource_type": "ulb",
                        "book_code": "luk",
                    },
                    {
                        "lang_code": "en",
                        "resource_type": "tn",
                        "book_code": "luk",
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
                ],
            },
        )
        check_finished_document_with_verses_success(response, suffix="pdf")


def test_pt_br_ulb_tn_luk_en_ulb_tn_luk_sw_ulb_tn_col_language_book_order_1c_c() -> (
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
                "generate_pdf": True,
                "generate_epub": False,
                "generate_docx": False,
                "resource_requests": [
                    {
                        "lang_code": "pt-br",
                        "resource_type": "ulb",
                        "book_code": "luk",
                    },
                    {
                        "lang_code": "pt-br",
                        "resource_type": "tn",
                        "book_code": "luk",
                    },
                    {
                        "lang_code": "en",
                        "resource_type": "ulb",
                        "book_code": "luk",
                    },
                    {
                        "lang_code": "en",
                        "resource_type": "tn",
                        "book_code": "luk",
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
                ],
            },
        )
        check_finished_document_with_verses_success(response, suffix="pdf")


def test_en_ulb_col_en_tn_col_en_tq_col_en_tw_col_sw_ulb_col_sw_tn_col_sw_tq_col_sw_tw_col_sw_ulb_tit_sw_tn_tit_sw_tq_tit_sw_tw_tit_language_book_order_1c() -> (
    None
):
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                # "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN,
                "assembly_layout_kind": None,
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
        check_finished_document_with_verses_success(response, suffix="pdf")


def test_en_ulb_col_en_tn_col_en_tq_col_en_tw_col_sw_ulb_col_sw_tn_col_sw_tq_col_sw_tw_col_sw_ulb_tit_sw_tn_tit_sw_tq_tit_sw_tw_tit_language_book_order_1c_c() -> (
    None
):
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                # "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN_COMPACT,
                "assembly_layout_kind": None,
                "layout_for_print": True,
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
        check_finished_document_with_verses_success(response, suffix="pdf")


def test_en_ulb_col_en_tn_col_en_tw_col_sw_ulb_col_sw_tn_col_sw_tw_col_sw_ulb_tit_sw_tn_tit_sw_tw_tit_language_book_order_1c() -> (
    None
):
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                # "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN,
                "assembly_layout_kind": None,
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
                        "resource_type": "tw",
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
                        "resource_type": "tw",
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
                        "resource_type": "tw",
                        "book_code": "tit",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response, suffix="pdf")


def test_en_ulb_col_en_tn_col_en_tw_col_sw_ulb_col_sw_tn_col_sw_tw_col_sw_ulb_tit_sw_tn_tit_sw_tw_tit_language_book_order_1c_c() -> (
    None
):
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                # "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN_COMPACT,
                "assembly_layout_kind": None,
                "layout_for_print": True,
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
                        "resource_type": "tw",
                        "book_code": "col",
                    },
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
                    {
                        "lang_code": "en",
                        "resource_type": "tw",
                        "book_code": "tit",
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
                        "resource_type": "tw",
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
                        "resource_type": "tw",
                        "book_code": "tit",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response, suffix="pdf")


def test_en_ulb_col_en_tw_col_sw_ulb_col_sw_tw_col_sw_ulb_tit_sw_tw_tit_language_book_order_1c() -> (
    None
):
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": None,
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
                        "resource_type": "tw",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "en",
                        "resource_type": "ulb",
                        "book_code": "tit",
                    },
                    {
                        "lang_code": "en",
                        "resource_type": "tw",
                        "book_code": "tit",
                    },
                    {
                        "lang_code": "sw",
                        "resource_type": "ulb",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "sw",
                        "resource_type": "tw",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "sw",
                        "resource_type": "ulb",
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
        check_finished_document_with_verses_success(response, suffix="pdf")


def test_en_ulb_col_en_tw_col_sw_ulb_col_sw_tw_col_sw_ulb_tit_sw_tw_tit_language_book_order_1c_c() -> (
    None
):
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                # "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN_COMPACT,
                "assembly_layout_kind": None,
                "layout_for_print": True,
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
                        "resource_type": "tw",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "en",
                        "resource_type": "ulb",
                        "book_code": "tit",
                    },
                    {
                        "lang_code": "en",
                        "resource_type": "tw",
                        "book_code": "tit",
                    },
                    {
                        "lang_code": "sw",
                        "resource_type": "ulb",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "sw",
                        "resource_type": "tw",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "sw",
                        "resource_type": "ulb",
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
        check_finished_document_with_verses_success(response, suffix="pdf")


def test_en_ulb_col_en_tq_col_en_tw_col_sw_ulb_col_sw_tq_col_sw_tw_col_sw_ulb_tit_sw_tq_tit_sw_tw_tit_language_book_order_1c() -> (
    None
):
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                # "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN,
                "assembly_layout_kind": None,
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
                        "resource_type": "tw",
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
                    {
                        "lang_code": "sw",
                        "resource_type": "tw",
                        "book_code": "tit",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response, suffix="pdf")


def test_en_ulb_col_en_tq_col_en_tw_col_sw_ulb_col_sw_tq_col_sw_tw_col_sw_ulb_tit_sw_tq_tit_sw_tw_tit_language_book_order_1c_c() -> (
    None
):
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                # "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN_COMPACT,
                "assembly_layout_kind": None,
                "layout_for_print": True,
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
                        "resource_type": "ulb",
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
                        "resource_type": "tw",
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
                    {
                        "lang_code": "sw",
                        "resource_type": "tw",
                        "book_code": "tit",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response, suffix="pdf")


# cuv is not provided by data api
@pytest.mark.skip
def test_en_ulb_col_en_tq_col_en_tw_col_sw_ulb_col_sw_tq_col_sw_tw_col_zh_cuv_tit_sw_tq_tit_sw_tw_tit_language_book_order_1c() -> (
    None
):
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": None,
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
                        "resource_type": "tw",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "zh",
                        "resource_type": "cuv",
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
        check_finished_document_with_verses_success(response, suffix="pdf")


def test_en_ulb_col_en_tq_col_en_tw_col_sw_ulb_col_sw_tq_col_sw_tw_col_zh_cuv_tit_sw_tq_tit_sw_tw_tit_language_book_order_1c_c() -> (
    None
):
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": None,
                "layout_for_print": True,
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
                        "resource_type": "tw",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "zh",
                        "resource_type": "cuv",
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
        check_finished_document_with_verses_success(response, suffix="pdf")


###################################################################
# Tests that originally were randomly chosen and failed
# using our random combinatoric tests.
###################################################################


# cuv is not provided by data api
@pytest.mark.skip
def test_zh_cuv_jol_zh_tn_jol_language_book_order_1c() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                # "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN,
                "assembly_layout_kind": None,
                "layout_for_print": False,
                "generate_pdf": True,
                "generate_epub": False,
                "generate_docx": False,
                "resource_requests": [
                    {
                        "lang_code": "zh",
                        "resource_type": "cuv",
                        "book_code": "jol",
                    },
                    {
                        "lang_code": "zh",
                        "resource_type": "tn",
                        "book_code": "jol",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response, suffix="pdf")


# cuv is not provided by data api
@pytest.mark.skip
def test_zh_cuv_jol_zh_tn_jol_language_book_order_1c_c() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": None,
                "layout_for_print": True,
                "generate_pdf": True,
                "generate_epub": False,
                "generate_docx": False,
                "resource_requests": [
                    {
                        "lang_code": "zh",
                        "resource_type": "cuv",
                        "book_code": "jol",
                    },
                    {
                        "lang_code": "zh",
                        "resource_type": "tn",
                        "book_code": "jol",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response, suffix="pdf")


# cuv is not provided by data api
@pytest.mark.skip
def test_zh_cuv_jol_zh_tn_jol_zh_tq_jol_zh_tw_jol_language_book_order_1c() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                # "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN,
                "assembly_layout_kind": None,
                "layout_for_print": False,
                "generate_pdf": True,
                "generate_epub": False,
                "generate_docx": False,
                "resource_requests": [
                    {
                        "lang_code": "zh",
                        "resource_type": "cuv",
                        "book_code": "jol",
                    },
                    {
                        "lang_code": "zh",
                        "resource_type": "tn",
                        "book_code": "jol",
                    },
                    {
                        "lang_code": "zh",
                        "resource_type": "tq",
                        "book_code": "jol",
                    },
                    {
                        "lang_code": "zh",
                        "resource_type": "tw",
                        "book_code": "jol",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response, suffix="pdf")


# cuv is not provided by data api
@pytest.mark.skip
@pytest.mark.docx
def test_zh_cuv_jol_zh_tn_jol_zh_tq_jol_zh_tw_jol_language_book_order_1c_docx() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents_docx",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                # "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN,
                "assembly_layout_kind": None,
                "layout_for_print": False,
                "generate_pdf": False,
                "generate_epub": False,
                "generate_docx": True,
                "resource_requests": [
                    {
                        "lang_code": "zh",
                        "resource_type": "cuv",
                        "book_code": "jol",
                    },
                    {
                        "lang_code": "zh",
                        "resource_type": "tn",
                        "book_code": "jol",
                    },
                    {
                        "lang_code": "zh",
                        "resource_type": "tq",
                        "book_code": "jol",
                    },
                    {
                        "lang_code": "zh",
                        "resource_type": "tw",
                        "book_code": "jol",
                    },
                ],
            },
        )
        check_result(response, suffix="docx")


# cuv is not provided by data api
@pytest.mark.skip
def test_zh_cuv_jol_zh_tn_jol_zh_tq_jol_zh_tw_jol_language_book_order_1c_c() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": None,
                "layout_for_print": True,
                "generate_pdf": True,
                "generate_epub": False,
                "generate_docx": False,
                "resource_requests": [
                    {
                        "lang_code": "zh",
                        "resource_type": "cuv",
                        "book_code": "jol",
                    },
                    {
                        "lang_code": "zh",
                        "resource_type": "tn",
                        "book_code": "jol",
                    },
                    {
                        "lang_code": "zh",
                        "resource_type": "tq",
                        "book_code": "jol",
                    },
                    {
                        "lang_code": "zh",
                        "resource_type": "tw",
                        "book_code": "jol",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response, suffix="pdf")


def test_pt_br_ulb_luk_pt_br_tn_luk_language_book_order_1c() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": None,
                "layout_for_print": False,
                "generate_pdf": True,
                "generate_epub": False,
                "generate_docx": False,
                "resource_requests": [
                    {
                        "lang_code": "pt-br",
                        "resource_type": "ulb",
                        "book_code": "luk",
                    },
                    {
                        "lang_code": "pt-br",
                        "resource_type": "tn",
                        "book_code": "luk",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response, suffix="pdf")


def test_pt_br_ulb_luk_pt_br_tn_luk_language_book_order_1c_c() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                # "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN_COMPACT,
                "assembly_layout_kind": None,
                "layout_for_print": True,
                "generate_pdf": True,
                "generate_epub": False,
                "generate_docx": False,
                "resource_requests": [
                    {
                        "lang_code": "pt-br",
                        "resource_type": "ulb",
                        "book_code": "luk",
                    },
                    {
                        "lang_code": "pt-br",
                        "resource_type": "tn",
                        "book_code": "luk",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response, suffix="pdf")


# [('fa',
#   [('tn', 'Translation Notes (tn)'),
#    ('ulb', 'Henry Martyn Open Source Bible (1876) (ulb)')]),
#  ('id',
#   [('tn', 'Translation Notes (tn)'),
#    ('tq', 'Translation Questions (tq)'),
#    ('tw', 'Translation Words (tw)')]),
#  ('pmy',
#   [('tn', 'Translation Notes (tn)'),
#    ('tq', 'Translation Questions (tq)'),
#    ('tw', 'Translation Words (tw)')])]
@pytest.mark.docx
def test_fa_ulb_tn_lbo_1c_chapter() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents_docx",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": None,
                "layout_for_print": False,
                "generate_pdf": False,
                "generate_epub": False,
                "generate_docx": True,
                "resource_requests": [
                    {
                        "lang_code": "fa",
                        "resource_type": "ulb",
                        "book_code": "eph",
                    },
                    {
                        "lang_code": "fa",
                        "resource_type": "tn",
                        "book_code": "eph",
                    },
                ],
            },
        )
        check_result(response, suffix="docx")


def test_or_tn_mat_lbo_1c_chapter() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                # "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN_COMPACT,
                "assembly_layout_kind": None,
                "layout_for_print": True,
                "generate_pdf": True,
                "generate_epub": False,
                "generate_docx": False,
                "resource_requests": [
                    {
                        "lang_code": "or",
                        "resource_type": "tn",
                        "book_code": "mat",
                    },
                ],
            },
        )
        check_finished_document_with_body_success(response, suffix="pdf")


# This language is no longer returned by the data API in the list of available languages.
@pytest.mark.skip
def test_nyk_x_nyanehumbe_reg_1pe_lbo_1c_chapter() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": None,
                "layout_for_print": False,
                "generate_pdf": True,
                "generate_epub": False,
                "generate_docx": False,
                "resource_requests": [
                    {
                        "lang_code": "nyk-x-nyanhumbe",
                        "resource_type": "reg",
                        "book_code": "1pe",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response, suffix="pdf")


def test_bjz_reg_eph_lbo_1c_chapter() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": None,
                "layout_for_print": False,
                "generate_pdf": True,
                "generate_epub": False,
                "generate_docx": False,
                "resource_requests": [
                    {
                        "lang_code": "bjz",
                        "resource_type": "reg",
                        "book_code": "eph",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response, suffix="pdf")


def test_bys_reg_col_lbo_1c_chapter() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": None,
                "layout_for_print": False,
                "generate_pdf": True,
                "generate_epub": False,
                "generate_docx": False,
                "resource_requests": [
                    {
                        "lang_code": "bys",
                        "resource_type": "reg",
                        "book_code": "col",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response, suffix="pdf")


def test_en_ulb_col_bys_reg_col_blo_1c_chapter() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_CHAPTER,
                "assembly_layout_kind": None,
                "layout_for_print": False,
                "generate_pdf": True,
                "generate_epub": False,
                "generate_docx": False,
                "resource_requests": [
                    {
                        "lang_code": "bys",
                        "resource_type": "reg",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "en",
                        "resource_type": "ulb",
                        "book_code": "col",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response, suffix="pdf")


def test_en_ulb_col_bjz_reg_col_blo_1c_chapter() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_CHAPTER,
                "assembly_layout_kind": None,
                "layout_for_print": False,
                "generate_pdf": True,
                "generate_epub": False,
                "generate_docx": False,
                "resource_requests": [
                    {
                        "lang_code": "bjz",
                        "resource_type": "reg",
                        "book_code": "col",
                    },
                    {
                        "lang_code": "en",
                        "resource_type": "ulb",
                        "book_code": "col",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response, suffix="pdf")


def test_en_ulb_eph_bys_reg_eph_blo_1c_chapter_docx() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents_docx",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_CHAPTER,
                "assembly_layout_kind": None,
                "layout_for_print": False,
                "generate_pdf": False,
                "generate_epub": False,
                "generate_docx": True,
                "resource_requests": [
                    {
                        "lang_code": "bys",
                        "resource_type": "reg",
                        "book_code": "eph",
                    },
                    {
                        "lang_code": "en",
                        "resource_type": "ulb",
                        "book_code": "eph",
                    },
                ],
            },
        )
        check_result(response, suffix="docx")


@pytest.mark.docx
def test_en_bc_col_language_book_order_with_no_email_1c_docx() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents_docx",
            json={
                # "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN,
                "layout_for_print": False,
                "generate_pdf": False,
                "generate_epub": False,
                "generate_docx": True,
                "resource_requests": [
                    {
                        "lang_code": "en",
                        "resource_type": "bc",
                        "book_code": "col",
                    },
                ],
            },
        )
        check_result(response, suffix="docx")


def test_en_bc_col_language_book_order_with_no_email_1c() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                # "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN,
                "layout_for_print": False,
                "generate_pdf": True,
                "generate_epub": False,
                "generate_docx": False,
                "resource_requests": [
                    {
                        "lang_code": "en",
                        "resource_type": "bc",
                        "book_code": "col",
                    },
                ],
            },
        )
        check_finished_document_without_verses_success(response, suffix="pdf")


@pytest.mark.skip
def test_en_ulb_1jn_en_ulb_3jn_language_book_order_with_no_email_1c() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                # "email_address": get_settings().TO_EMAIL_ADDRESS,
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
                        "book_code": "1jn",
                    },
                    {
                        "lang_code": "en",
                        "resource_type": "ulb",
                        "book_code": "3jn",
                    },
                ],
            },
        )
        finished_document_request_key = check_result(
            response, suffix="html", poll_duration=4
        )
        html_filepath = os.path.join(
            get_settings().DOCUMENT_OUTPUT_DIR,
            "{}.html".format(finished_document_request_key),
        )
        with open(html_filepath, "r") as fin:
            html = fin.read()
            body_match = re.search(r"<body.*?>(.*?)</body>", html, re.DOTALL)
            assert body_match, "Body not found in HTML"
            body_content = body_match.group(1)
            assert (
                "1 John</h2>" in body_content and "3 John</h2>" in body_content
            ), "Document should have had both 1 John and 3 John in it, but it didn't"


@pytest.mark.skip
def test_en_tn_mat_with_book_intro() -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                # "email_address": get_settings().TO_EMAIL_ADDRESS,
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
                        "book_code": "mat",
                    },
                ],
            },
        )
        finished_document_request_key = check_result(
            response, suffix="html", poll_duration=4
        )
        html_filepath = os.path.join(
            get_settings().DOCUMENT_OUTPUT_DIR,
            "{}.html".format(finished_document_request_key),
        )
        with open(html_filepath, "r") as fin:
            html = fin.read()
            body_match = re.search(r"<body.*?>(.*?)</body>", html, re.DOTALL)
            assert body_match, "Body not found in HTML"
            body_content = body_match.group(1)
            if get_settings().SHOW_TN_BOOK_INTRO:
                assert (
                    "Outline of the Book of Matthew" in body_content
                ), "Document should have had TN book intro for mat, but didn't"
