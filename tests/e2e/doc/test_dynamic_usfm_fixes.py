"""
This module tests languages which were found through automatic randomized testing and subsequent manual investigation to have certain potentially fixable, on the fly, USFM defects that prevent them from being parsed.
"""

from typing import Sequence
import pytest
from doc.config import get_settings
from doc.domain import (
    model,
    exceptions,
    resource_lookup,
    usfm_error_detection_and_fixes,
)
from doc.entrypoints.app import app
from fastapi.testclient import TestClient

from tests.shared.utils import check_finished_document_with_verses_success


logger = get_settings().logger(__name__)


def languages_with_usfm_defects(
    resources_with_usfm_defects: Sequence[
        tuple[str, str, str]
    ] = usfm_error_detection_and_fixes.RESOURCES_WITH_USFM_DEFECTS,
) -> list[str]:
    lang_codes = [resource_tuple[0] for resource_tuple in resources_with_usfm_defects]
    return sorted(set(lang_codes))


def resources_with_usfm_defects_(
    resources_with_usfm_defects: Sequence[
        tuple[str, str, str]
    ] = usfm_error_detection_and_fixes.RESOURCES_WITH_USFM_DEFECTS,
) -> Sequence[tuple[str, str, str]]:
    return resources_with_usfm_defects


# Test programmatically checking all books of languages known to have USFM defects
@pytest.mark.skip
@pytest.mark.usfm_fixes
@pytest.mark.parametrize(
    "lang_code",
    languages_with_usfm_defects(),
)
def test_all_usfm_books_for_language(lang_code: str) -> None:
    with TestClient(app=app, base_url=get_settings().api_test_url()) as client:
        book_codes_and_names = resource_lookup.book_codes_for_lang_from_usfm_only(
            lang_code
        )
        if not book_codes_and_names:
            raise exceptions.NoBooksError(message="no available books.")
        usfm_resource_types_and_book_tuples_ = (
            resource_lookup.usfm_resource_types_and_book_tuples(
                lang_code,
                ",".join(
                    [
                        book_code_and_name[0]
                        for book_code_and_name in book_codes_and_names
                    ]
                ),
            )
        )
        # logger.debug(
        #     "usfm_resource_types_and_book_tuples_: %s",
        #     usfm_resource_types_and_book_tuples_,
        # )
        for usfm_resource_types_and_book_tuple in usfm_resource_types_and_book_tuples_:
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
                            "lang_code": lang_code,
                            "resource_type": usfm_resource_types_and_book_tuple[0],
                            "book_code": usfm_resource_types_and_book_tuple[1],
                        },
                    ],
                },
            )
            check_finished_document_with_verses_success(response, suffix="pdf")


@pytest.mark.usfm_fixes
@pytest.mark.parametrize(
    "lang_code, resource_type, book_code",
    resources_with_usfm_defects_(),
)
def test_known_defective_usfm_cases(
    lang_code: str, resource_type: str, book_code: str
) -> None:
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
                        "lang_code": lang_code,
                        "resource_type": resource_type,
                        "book_code": book_code,
                    },
                ],
            },
        )
        check_finished_document_with_verses_success(response, suffix="pdf")
