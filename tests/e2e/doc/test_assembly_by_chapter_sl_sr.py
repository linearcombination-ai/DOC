import pytest
from doc.config import get_settings
from doc.domain import model
from doc.entrypoints.app import app
from fastapi.testclient import TestClient

from tests.shared.utils import check_finished_document_with_verses_success

logger = get_settings().logger(__name__)

pytest.skip("Skipping this module", allow_module_level=True)


def test_en_ulb_col_en_tn_col_en_tq_col_en_tw_col_fr_f10_col_fr_tn_col_fr_tq_col_fr_tw_col_by_chapter_2c_sl_sr() -> (
    None
):
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_CHAPTER,
                "assembly_layout_kind": model.AssemblyLayoutEnum.TWO_COLUMN_SCRIPTURE_LEFT_SCRIPTURE_RIGHT,
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


def test_en_ulb_col_en_tn_col_en_tq_col_en_tw_col_fr_f10_col_fr_tn_col_fr_tq_col_fr_tw_col_by_chapter_2c_sl_sr_c() -> (
    None
):
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_CHAPTER,
                "assembly_layout_kind": model.AssemblyLayoutEnum.TWO_COLUMN_SCRIPTURE_LEFT_SCRIPTURE_RIGHT_COMPACT,
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


def test_en_ulb_col_en_tn_col_en_tq_col_en_tw_col_pt_br_ulb_col_pt_br_tn_col_pt_br_tq_col_pt_br_tw_col_by_chapter_2c_sl_sr() -> (
    None
):
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_CHAPTER,
                "assembly_layout_kind": model.AssemblyLayoutEnum.TWO_COLUMN_SCRIPTURE_LEFT_SCRIPTURE_RIGHT,
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


def test_en_ulb_col_en_tn_col_en_tq_col_en_tw_col_pt_br_ulb_col_pt_br_tn_col_pt_br_tq_col_pt_br_tw_col_by_chapter_2c_sl_sr_epub() -> (
    None
):
    with TestClient(app=app) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_CHAPTER,
                "assembly_layout_kind": model.AssemblyLayoutEnum.TWO_COLUMN_SCRIPTURE_LEFT_SCRIPTURE_RIGHT,
                "layout_for_print": False,
                "generate_pdf": False,
                "generate_epub": True,
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
        check_finished_document_with_verses_success(response, suffix="epub")


def test_en_ulb_col_en_tn_col_en_tq_col_en_tw_col_pt_br_ulb_col_pt_br_tn_col_pt_br_tq_col_pt_br_tw_col_by_chapter_2c_sl_sr_c() -> (
    None
):
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_CHAPTER,
                "assembly_layout_kind": model.AssemblyLayoutEnum.TWO_COLUMN_SCRIPTURE_LEFT_SCRIPTURE_RIGHT_COMPACT,
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


def test_en_ulb_col_en_tn_col_en_tq_col_en_tw_col_tl_ulb_col_tl_tn_col_tl_tq_col_tl_tw_col_tl_udb_col_by_chapter_2c_sl_sr() -> (
    None
):
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_CHAPTER,
                "assembly_layout_kind": model.AssemblyLayoutEnum.TWO_COLUMN_SCRIPTURE_LEFT_SCRIPTURE_RIGHT,
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
                ],
            },
        )
        check_finished_document_with_verses_success(response)


def test_en_ulb_col_en_tn_col_en_tq_col_en_tw_col_tl_ulb_col_tl_tn_col_tl_tq_col_tl_tw_col_tl_udb_col_by_chapter_2c_sl_sr_c() -> (
    None
):
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_CHAPTER,
                "assembly_layout_kind": model.AssemblyLayoutEnum.TWO_COLUMN_SCRIPTURE_LEFT_SCRIPTURE_RIGHT_COMPACT,
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


def test_en_ulb_col_en_tn_col_sw_ulb_col_sw_tn_col_sw_ulb_tit_sw_tn_tit_by_chapter_2c_sl_sr() -> (
    None
):
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_CHAPTER,
                "assembly_layout_kind": None,
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


def test_en_ulb_col_en_tn_col_sw_ulb_col_sw_tn_col_sw_ulb_tit_sw_tn_tit_by_chapter_2c_sl_sr_c() -> (
    None
):
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_CHAPTER,
                "assembly_layout_kind": None,
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


def test_en_ulb_col_en_tn_col_en_tq_col_sw_ulb_col_sw_tn_col_sw_tq_col_sw_ulb_tit_sw_tn_tit_sw_tq_tit_by_chapter_2c_sl_sr_c() -> (
    None
):
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_CHAPTER,
                "assembly_layout_kind": None,
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


def test_en_ulb_col_en_tn_col_en_tq_col_en_tw_col_es_419_ulb_col_es_419_tn_col_es_419_tq_col_es_419_tw_col_by_chapter_2c_sl_sr() -> (
    None
):
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_CHAPTER,
                "assembly_layout_kind": model.AssemblyLayoutEnum.TWO_COLUMN_SCRIPTURE_LEFT_SCRIPTURE_RIGHT,
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


def test_es_419_ulb_rom_es_419_tn_rom_es_419_tq_rom_es_419_tw_rom_pt_br_ulb_rom_pt_br_tn_rom_pt_br_tq_rom_pt_br_tw_rom_by_chapter_2c_sl_sr() -> (
    None
):
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_CHAPTER,
                "assembly_layout_kind": model.AssemblyLayoutEnum.TWO_COLUMN_SCRIPTURE_LEFT_SCRIPTURE_RIGHT,
                "layout_for_print": False,
                "generate_pdf": False,
                "generate_epub": False,
                "generate_docx": False,
                "resource_requests": [
                    {
                        "lang_code": "es-419",
                        "resource_type": "ulb",
                        "book_code": "rom",
                    },
                    {
                        "lang_code": "es-419",
                        "resource_type": "tn",
                        "book_code": "rom",
                    },
                    {
                        "lang_code": "es-419",
                        "resource_type": "tq",
                        "book_code": "rom",
                    },
                    {
                        "lang_code": "es-419",
                        "resource_type": "tw",
                        "book_code": "rom",
                    },
                    {
                        "lang_code": "pt-br",
                        "resource_type": "ulb",
                        "book_code": "rom",
                    },
                    {
                        "lang_code": "pt-br",
                        "resource_type": "tn",
                        "book_code": "rom",
                    },
                    {
                        "lang_code": "pt-br",
                        "resource_type": "tq",
                        "book_code": "rom",
                    },
                    {
                        "lang_code": "pt-br",
                        "resource_type": "tw",
                        "book_code": "rom",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response)


def test_es_419_ulb_rom_es_419_tn_rom_es_419_tq_rom_es_419_tw_rom_pt_br_ulb_rom_pt_br_tn_rom_pt_br_tq_rom_pt_br_tw_rom_by_chapter_2c_sl_sr_c() -> (
    None
):
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_CHAPTER,
                "assembly_layout_kind": model.AssemblyLayoutEnum.TWO_COLUMN_SCRIPTURE_LEFT_SCRIPTURE_RIGHT_COMPACT,
                "layout_for_print": False,
                "generate_pdf": False,
                "generate_epub": False,
                "generate_docx": False,
                "resource_requests": [
                    {
                        "lang_code": "es-419",
                        "resource_type": "ulb",
                        "book_code": "rom",
                    },
                    {
                        "lang_code": "es-419",
                        "resource_type": "tn",
                        "book_code": "rom",
                    },
                    {
                        "lang_code": "es-419",
                        "resource_type": "tq",
                        "book_code": "rom",
                    },
                    {
                        "lang_code": "es-419",
                        "resource_type": "tw",
                        "book_code": "rom",
                    },
                    {
                        "lang_code": "pt-br",
                        "resource_type": "ulb",
                        "book_code": "rom",
                    },
                    {
                        "lang_code": "pt-br",
                        "resource_type": "tn",
                        "book_code": "rom",
                    },
                    {
                        "lang_code": "pt-br",
                        "resource_type": "tq",
                        "book_code": "rom",
                    },
                    {
                        "lang_code": "pt-br",
                        "resource_type": "tw",
                        "book_code": "rom",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response)


def test_en_ulb_rom_en_tn_rom_en_tq_rom_en_tw_rom_zh_cuv_rom_zh_cuv_tn_rom_zh_cuv_tq_rom_zh_cuv_tw_rom_by_chapter_2c_sl_sr() -> (
    None
):
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_CHAPTER,
                "assembly_layout_kind": model.AssemblyLayoutEnum.TWO_COLUMN_SCRIPTURE_LEFT_SCRIPTURE_RIGHT,
                "layout_for_print": False,
                "generate_pdf": False,
                "generate_epub": False,
                "generate_docx": False,
                "resource_requests": [
                    {
                        "lang_code": "en",
                        "resource_type": "ulb",
                        "book_code": "rom",
                    },
                    {
                        "lang_code": "en",
                        "resource_type": "tn",
                        "book_code": "rom",
                    },
                    {
                        "lang_code": "en",
                        "resource_type": "tq",
                        "book_code": "rom",
                    },
                    {
                        "lang_code": "en",
                        "resource_type": "tw",
                        "book_code": "rom",
                    },
                    {
                        "lang_code": "zh",
                        "resource_type": "cuv",
                        "book_code": "rom",
                    },
                    {
                        "lang_code": "zh",
                        "resource_type": "tn",
                        "book_code": "rom",
                    },
                    {
                        "lang_code": "zh",
                        "resource_type": "tq",
                        "book_code": "rom",
                    },
                    {
                        "lang_code": "zh",
                        "resource_type": "tw",
                        "book_code": "rom",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response)


def test_en_ulb_rom_en_tn_rom_en_tq_rom_en_tw_rom_zh_cuv_rom_zh_cuv_tn_rom_zh_cuv_tq_rom_zh_cuv_tw_rom_by_chapter_2c_sl_sr_c() -> (
    None
):
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        response = client.post(
            "/documents",
            json={
                "email_address": get_settings().TO_EMAIL_ADDRESS,
                "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_CHAPTER,
                "assembly_layout_kind": model.AssemblyLayoutEnum.TWO_COLUMN_SCRIPTURE_LEFT_SCRIPTURE_RIGHT_COMPACT,
                "layout_for_print": False,
                "generate_pdf": False,
                "generate_epub": False,
                "generate_docx": False,
                "resource_requests": [
                    {
                        "lang_code": "en",
                        "resource_type": "ulb",
                        "book_code": "rom",
                    },
                    {
                        "lang_code": "en",
                        "resource_type": "tn",
                        "book_code": "rom",
                    },
                    {
                        "lang_code": "en",
                        "resource_type": "tq",
                        "book_code": "rom",
                    },
                    {
                        "lang_code": "en",
                        "resource_type": "tw",
                        "book_code": "rom",
                    },
                    {
                        "lang_code": "zh",
                        "resource_type": "cuv",
                        "book_code": "rom",
                    },
                    {
                        "lang_code": "zh",
                        "resource_type": "tn",
                        "book_code": "rom",
                    },
                    {
                        "lang_code": "zh",
                        "resource_type": "tq",
                        "book_code": "rom",
                    },
                    {
                        "lang_code": "zh",
                        "resource_type": "tw",
                        "book_code": "rom",
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response)
