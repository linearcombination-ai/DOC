"""
Useful to unearth combinations that (possibly) do not work (because
no test has been run on them until now). Such failing tests can then
be added as permanent non-random tests so that regressions are
avoided.

Since randomization is involved with respect to fixture combinations,
it could generate invalid document requests instances that fail their
model validation or may access resources which translators have not
yet prepared to be in the required formats which will then
cause the whole test suite to fail (depending on pytest config).
Therefore these tests are marked with the custom pytest marker
'randomized' to run them separately from the other tests. Note that
tests exist in additional places that could overlap with these so you
aren't not testing these at all by skipping this in the main test run.
This is just additional coverage that could be run once in a while
using the 'randomized' pytest marker.
"""

import pytest
import random
from doc.config import settings
from doc.domain.constants import USFM_RESOURCE_TYPES
from doc.domain import exceptions
from doc.entrypoints.app import app
from fastapi.testclient import TestClient

from doc.domain import model, resource_lookup
from tests.shared.utils import (
    check_finished_document_with_body_success,
    check_finished_document_with_verses_success,
    check_finished_document_without_verses_success,
    # check_result,
)

logger = settings.logger(__name__)


@pytest.mark.parametrize("execution_number", range(25))
@pytest.mark.randomized
def test_random_non_english_document_request(
    execution_number: int,
    random_non_english_lang_code: str,
) -> None:
    """
    Use the fixtures in ./conftest.py for non-English languages to
    generate a random document request requested with resources
    dynamically requested each time this is run.
    """
    with TestClient(app=app, base_url=settings.api_test_url()) as client:
        book_codes_and_names = resource_lookup.book_codes_for_lang(
            random_non_english_lang_code
        )
        if not book_codes_and_names:
            raise exceptions.NoBooksError(
                message=f"{random_non_english_lang_code} has no available books."
            )
        book_code_and_name = random.choice(book_codes_and_names)
        resource_types_and_names = resource_lookup.resource_types(
            random_non_english_lang_code, book_code_and_name[0]
        )
        resource_requests: list[model.ResourceRequest] = []
        for resource_type_and_name in resource_types_and_names:
            resource_requests.append(
                model.ResourceRequest(
                    lang_code=random_non_english_lang_code,
                    resource_type=resource_type_and_name[0],
                    book_code=book_code_and_name[0],
                )
            )
        data = model.DocumentRequest(
            email_address=settings.TO_EMAIL_ADDRESS,
            assembly_strategy_kind=model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
            assembly_layout_kind=model.AssemblyLayoutEnum.ONE_COLUMN,
            layout_for_print=False,
            generate_pdf=True,
            generate_epub=False,
            generate_docx=False,
            resource_requests=resource_requests,
        )
        logger.debug("json_request: %s", data.model_dump_json(indent=4))
        response = client.post("/documents", json=data.dict())
        assert response.status_code == 200
        if any(
            resource_request.resource_type in USFM_RESOURCE_TYPES
            for resource_request in resource_requests
        ):
            check_finished_document_with_verses_success(response)
        else:
            check_finished_document_with_body_success(response)
            check_finished_document_without_verses_success(response)


@pytest.mark.all
def test_non_english_usfm_only_document_request(
    non_english_lang_code: str,
) -> None:
    """
    Use the fixtures in ./conftest.py for non-English languages to
    generate a random document request requested with resources
    dynamically requested each time this is run.
    """
    with TestClient(app=app, base_url=settings.api_test_url()) as client:
        book_codes_and_names = resource_lookup.book_codes_for_lang(
            non_english_lang_code
        )
        if not book_codes_and_names:
            raise exceptions.NoBooksError(
                message=f"{non_english_lang_code} has no available books."
            )
        usfm_resource_types_and_book_tuples_ = (
            resource_lookup.usfm_resource_types_and_book_tuples(
                non_english_lang_code,
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
                    "email_address": settings.TO_EMAIL_ADDRESS,
                    "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                    "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN,
                    "layout_for_print": False,
                    "generate_pdf": True,
                    "generate_epub": False,
                    "generate_docx": False,
                    "resource_requests": [
                        {
                            "lang_code": non_english_lang_code,
                            "resource_type": usfm_resource_types_and_book_tuple[0],
                            "book_code": usfm_resource_types_and_book_tuple[1],
                        },
                    ],
                },
            )
            check_finished_document_with_verses_success(response, suffix="pdf")


@pytest.mark.parametrize("execution_number", range(25))
@pytest.mark.randomized
def test_random_english_and_non_english_combo_document_request(
    execution_number: int,
    random_english_and_non_english_document_request: model.DocumentRequest,
) -> None:
    """
    Use the fixtures in ./conftest.py for English and non-English languages to
    generate a random multi-language document request each time this is run.
    """
    data = random_english_and_non_english_document_request.dict()
    with TestClient(app=app, base_url=settings.api_test_url()) as client:
        response = client.post("/documents", json=data)
        assert response.status_code == 200
        if any(
            resource_request.resource_type in USFM_RESOURCE_TYPES
            for resource_request in random_english_and_non_english_document_request.resource_requests
        ):
            check_finished_document_with_verses_success(response)
        else:
            check_finished_document_with_body_success(response)
            check_finished_document_without_verses_success(response)


@pytest.mark.parametrize("execution_number", range(25))
@pytest.mark.randomized
def test_random_two_non_english_languages_combo_document_request(
    execution_number: int,
    random_two_non_english_languages_document_request: model.DocumentRequest,
) -> None:
    """
    Use the fixtures in ./conftest.py for two non-English languages to
    generate a random multi-language document request each time this is run.
    """
    data = random_two_non_english_languages_document_request.dict()
    with TestClient(app=app, base_url=settings.api_test_url()) as client:
        response = client.post("/documents", json=data)
        assert response.status_code == 200
        if any(
            resource_request.resource_type in USFM_RESOURCE_TYPES
            for resource_request in random_two_non_english_languages_document_request.resource_requests
        ):
            check_finished_document_with_verses_success(response)
        else:
            check_finished_document_with_body_success(response)
            check_finished_document_without_verses_success(response)


@pytest.mark.skip
@pytest.mark.all
def test_all_non_english_document_request(non_english_lang_code: str) -> None:
    """
    Test all languages, all their books, and all their resource types.
    """
    with TestClient(app=app, base_url=settings.api_test_url()) as client:
        book_codes_and_names = resource_lookup.book_codes_for_lang(
            non_english_lang_code
        )
        if not book_codes_and_names:
            raise exceptions.NoBooksError(
                message=f"{non_english_lang_code} has no available books."
            )
        for book_code_and_name in book_codes_and_names:
            resource_types_and_names = resource_lookup.resource_types(
                non_english_lang_code, book_code_and_name[0]
            )
            resource_requests: list[model.ResourceRequest] = []
            for resource_type_and_name in resource_types_and_names:
                resource_requests.append(
                    model.ResourceRequest(
                        lang_code=non_english_lang_code,
                        resource_type=resource_type_and_name[0],
                        book_code=book_code_and_name[0],
                    )
                )
            data = model.DocumentRequest(
                email_address=settings.TO_EMAIL_ADDRESS,
                assembly_strategy_kind=model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                assembly_layout_kind=model.AssemblyLayoutEnum.ONE_COLUMN,
                layout_for_print=False,
                generate_pdf=True,
                generate_epub=False,
                generate_docx=False,
                resource_requests=resource_requests,
            )
            logger.debug("json_request: %s", data.model_dump_json(indent=4))
            response = client.post("/documents", json=data.dict())
            assert response.status_code == 200
            if any(
                resource_request.resource_type in USFM_RESOURCE_TYPES
                for resource_request in resource_requests
            ):
                check_finished_document_with_verses_success(response)
            else:
                check_finished_document_with_body_success(response)
                check_finished_document_without_verses_success(response)
