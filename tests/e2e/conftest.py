"""This module provides fixtures for e2e tests."""

import itertools
import random
from typing import Any, Optional, Sequence

import pydantic
import pytest
from doc.config import settings
from doc.domain.constants import USFM_RESOURCE_TYPES
from doc.domain import bible_books, exceptions, model, resource_lookup

logger = settings.logger(__name__)

lang_codes_and_names = resource_lookup.lang_codes_and_names()
ALL_LANGUAGE_CODES: Sequence[str] = [
    lang_code_and_name[0] for lang_code_and_name in lang_codes_and_names
]
# logger.debug("len(ALL_LANGUAGE_CODES): %s", len(ALL_LANGUAGE_CODES)) # result: 763


@pytest.fixture()
def english_lang_code() -> str:
    return "en"


# There are so many language codes, so we limit the number we test in a test run
# to the next interval yet untested until finally this will be adjusted to have
# tested them all: 764, i.e., 0 to 763.
# Type of request parameter is actually _pytest.fixtures.FixtureRequest
@pytest.fixture(params=ALL_LANGUAGE_CODES[700:763])
def non_english_lang_code(request: Any) -> Any:
    """Get all non-English language codes, but one per request."""
    return request.param


@pytest.fixture()
def random_non_english_lang_code() -> str:
    """
    Return a randomly chosen non-English language code that we
    want to test.
    """
    return random.choice(ALL_LANGUAGE_CODES)


@pytest.fixture()
def random_non_english_lang_code2() -> str:
    """
    Return a randomly chosen non-English language code that we
    want to test. This fixture is used when we want a second random
    non-English language code for the case where we want a document
    request with more than one non-English language in it.
    """
    return random.choice(ALL_LANGUAGE_CODES)


@pytest.fixture(params=bible_books.BOOK_NAMES.keys())
def book_code(request: Any) -> Any:
    """All book codes sequentially, but one at a time."""
    return request.param


# @pytest.fixture()
# def random_book_code() -> str:
#     """One book code chosen at random."""
#     book_codes: list[str] = list(bible_books.BOOK_NAMES.keys())
#     return random.choice(book_codes)


# @pytest.fixture()
# def random_book_code2() -> str:
#     """One book code chosen at random. This fixture exists so
#     that we can have a separate book chosen in a two language document
#     request."""
#     book_codes: list[str] = list(bible_books.BOOK_NAMES.keys())
#     return random.choice(book_codes)


@pytest.fixture()
def email_address() -> str:
    return str(settings.TO_EMAIL_ADDRESS)


@pytest.fixture()
def assembly_strategy_kind() -> str:
    return str(
        random.choice(
            [
                "lbo",
                "blo",
            ]
        )
    )


@pytest.fixture
def assembly_layout_kind() -> Optional[str]:
    return None  # Choose none to let the system decide


@pytest.fixture
def layout_for_print() -> bool:
    return random.choice([True, False])


@pytest.fixture
def generate_pdf() -> bool:
    return random.choice([True, False])


@pytest.fixture
def generate_epub() -> bool:
    return random.choice([True, False])


@pytest.fixture
def generate_docx() -> bool:
    return random.choice([True, False])


@pytest.fixture()
def english_resource_types() -> Sequence[str]:
    """All the English resource types."""
    # return ["ulb", "tn", "tq", "tw", "bc"]
    # tq is not currently available through the graphql api
    return ["ulb", "tn", "tn-condensed", "tw", "bc"]


# @pytest.fixture()
# def non_english_resource_types() -> Sequence[str]:
#     """Most common non-English resource types."""
#     return ["ulb", "reg", "tn", "tq", "tw"]


@pytest.fixture()
def english_resource_type_combos(
    english_resource_types: Sequence[str],
) -> Sequence[tuple[str, ...]]:
    """
    All possible combinations, in the mathematical sense, of
    English resource types. See documentation for
    itertools.combinations for the combinatoric algorithm.
    """
    resource_type_combos = []
    for idx in range(1, len(english_resource_types)):
        resource_type_combos.extend(
            list(itertools.combinations(english_resource_types, idx))
        )
    return resource_type_combos


# @pytest.fixture()
# def non_english_resource_type_combos(
#     non_english_resource_types: Sequence[str],
# ) -> Sequence[tuple[str, ...]]:
#     """
#     All possible combinations, in the mathematical sense, of
#     non-English resource types. See documentation for
#     itertools.combinations for the combinatoric algorithm.
#     """
#     resource_type_combos = []
#     for idx in range(1, len(non_english_resource_types)):
#         resource_type_combos.extend(
#             list(itertools.combinations(non_english_resource_types, idx))
#         )
#     return resource_type_combos


@pytest.fixture()
def random_english_resource_type_combo(
    english_resource_type_combos: Sequence[Sequence[str]],
) -> Sequence[str]:
    """
    A random choice of one set of all possible English resource type
    combination sets, e.g., ulb, tn, tw; or, ulb, tn,
    tw; or ulb, tn, tq, tw; etc..
    """
    return random.choice(english_resource_type_combos)


# @pytest.fixture()
# def random_non_english_resource_type_combo(
#     non_english_resource_type_combos: Sequence[Sequence[str]],
# ) -> Sequence[str]:
#     """
#     A random choice of one set of all possible non-English resource type
#     combination sets, e.g., ulb, tn, tw; or, ulb, tn, tw; or ulb, tn,
#     tq, tw; etc..
#     """
#     return random.choice(non_english_resource_type_combos)


@pytest.fixture()
def english_resource_requests(
    english_lang_code: str,
    random_english_resource_type_combo: Sequence[str],
    book_code: str,
) -> Sequence[model.ResourceRequest]:
    """
    Build a list of resource request instances for the set of English
    resource types passed in as a parameter and a book_code. This
    will cycle through all book_codes.
    """
    resource_requests = [
        model.ResourceRequest(
            lang_code=english_lang_code,
            resource_type=resource_type,
            book_code=book_code,
        )
        for resource_type in random_english_resource_type_combo
    ]
    return resource_requests


# @pytest.fixture()
# def random_english_resource_requests(
#     english_lang_code: str,
# ) -> list[model.ResourceRequest]:
#     """
#     Build a list of resource request instances for the set of English
#     resource types passed in as a parameter and a randomly chosen
#     resource code.
#     """
#     book_codes_and_names = resource_lookup.book_codes_for_lang(english_lang_code)
#     book_code_and_name = random.choice(book_codes_and_names)
#     resource_types_and_names = resource_lookup.resource_types(english_lang_code)
#     resource_requests: list[model.ResourceRequest] = []
#     for resource_type_and_name in resource_types_and_names:
#         resource_requests.append(
#             model.ResourceRequest(
#                 lang_code=english_lang_code,
#                 resource_type=resource_type_and_name[0],
#                 book_code=book_code_and_name[0],
#             )
#         )
#     return resource_requests


# @pytest.fixture()
# def random_non_english_resource_requests(
#     random_non_english_lang_code: str,
# ) -> list[model.ResourceRequest]:
#     """
#     Build a list of resource request instances for a randomly chosen
#     non-English lang_code, the set of non-English resource types
#     passed in as a parameter, and a randomly chosen resource code.
#     """
#     book_codes_and_names = resource_lookup.book_codes_for_lang(
#         random_non_english_lang_code
#     )
#     if not book_codes_and_names:
#         raise exceptions.NoBooksError(
#             message=f"{random_non_english_lang_code} has no available books."
#         )
#     book_code_and_name = random.choice(book_codes_and_names)
#     resource_types_and_names = resource_lookup.resource_types(
#         random_non_english_lang_code
#     )
#     resource_requests: list[model.ResourceRequest] = []
#     for resource_type_and_name in resource_types_and_names:
#         resource_requests.append(
#             model.ResourceRequest(
#                 lang_code=random_non_english_lang_code,
#                 resource_type=resource_type_and_name[0],
#                 book_code=book_code_and_name[0],
#             )
#         )
#     return resource_requests


@pytest.fixture()
def english_document_request(
    email_address: pydantic.EmailStr,
    assembly_layout_kind: model.AssemblyLayoutEnum,
    layout_for_print: bool,
    generate_pdf: bool,
    generate_epub: bool,
    generate_docx: bool,
    english_resource_requests: Sequence[model.ResourceRequest],
) -> model.DocumentRequest:
    """Build one English language document request."""
    return model.DocumentRequest(
        email_address=email_address,
        assembly_strategy_kind=model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
        assembly_layout_kind=assembly_layout_kind,
        layout_for_print=layout_for_print,
        generate_pdf=generate_pdf,
        generate_epub=generate_epub,
        generate_docx=generate_docx,
        resource_requests=english_resource_requests,
    )


# @pytest.fixture()
# def random_non_english_document_request(
#     email_address: pydantic.EmailStr,
#     assembly_layout_kind: model.AssemblyLayoutEnum,
#     layout_for_print: bool,
#     generate_pdf: bool,
#     generate_epub: bool,
#     generate_docx: bool,
#     random_non_english_resource_requests: Sequence[model.ResourceRequest],
# ) -> model.DocumentRequest:
#     """
#     Build one non-English language document request.

#     NOTE Many such randomly generated non-English tests will fail
#     since non-English language support is not complete with respect to
#     resource types or books. Thus we can use this test to find tests
#     that we expect to fail and possibly use such tests to identify
#     language-resource_type-book_code combos that should be
#     precluded from the front end so as not to waste user's time
#     requesting a document that cannot be successfully fulfilled. Or,
#     short of that, to help guide us to implementing the graceful
#     raising of exceptions and their handlers for such failures.
#     """
#     return model.DocumentRequest(
#         email_address=email_address,
#         assembly_strategy_kind=model.AssemblyStrategyEnum.LANGUAGE_BOOK_ORDER,
#         assembly_layout_kind=assembly_layout_kind,
#         layout_for_print=layout_for_print,
#         generate_pdf=generate_pdf,
#         generate_epub=generate_epub,
#         generate_docx=generate_docx,
#         resource_requests=random_non_english_resource_requests,
#     )


## Multi-language combination fixtures start here:


@pytest.fixture()
def random_english_and_non_english_document_request(
    email_address: pydantic.EmailStr,
    assembly_layout_kind: model.AssemblyLayoutEnum,
    layout_for_print: bool,
    generate_pdf: bool,
    generate_epub: bool,
    generate_docx: bool,
    english_lang_code: str,
    random_non_english_lang_code: str,
) -> model.DocumentRequest:
    """
    Build one document request having resource requests for English and
    one non-English language.

    NOTE Many such randomly generated non-English tests will fail
    since non-English language support is not complete with respect to
    resource types or books. Thus we can use this test to find tests
    that we expect to fail and possibly use such tests to identify
    language-resource_type-book_code combos that should be
    precluded from the front end so as not to waste user's time
    requesting a document that cannot be successfully fulfilled. Or,
    short of that, to help guide us to implementing the graceful
    raising of exceptions and their handlers for such failures.
    """
    book_codes_and_names = resource_lookup.book_codes_for_lang(english_lang_code)
    if not book_codes_and_names:
        raise exceptions.NoBooksError(
            message=f"{english_lang_code} has no available books."
        )
    book_code_and_name = random.choice(book_codes_and_names)
    resource_types_and_names = resource_lookup.resource_types(
        english_lang_code, book_code_and_name[0]
    )
    resource_requests: list[model.ResourceRequest] = []
    for resource_type_and_name in resource_types_and_names:
        resource_requests.append(
            model.ResourceRequest(
                lang_code=english_lang_code,
                resource_type=resource_type_and_name[0],
                book_code=book_code_and_name[0],
            )
        )
    # Language 2
    book_codes_and_names2 = resource_lookup.book_codes_for_lang(
        random_non_english_lang_code
    )
    if not book_codes_and_names2:
        raise exceptions.NoBooksError(
            message=f"{random_non_english_lang_code} has no available books."
        )
    resource_types_and_names2 = resource_lookup.resource_types(
        random_non_english_lang_code,
        ",".join(
            [book_code_and_name[0] for book_code_and_name in book_codes_and_names2]
        ),
    )
    for resource_type_and_name2 in resource_types_and_names2:
        if (
            resource_type_and_name2 in resource_types_and_names
            and book_code_and_name in book_codes_and_names2
        ):
            resource_requests.append(
                model.ResourceRequest(
                    lang_code=random_non_english_lang_code,
                    resource_type=resource_type_and_name2[0],
                    book_code=book_code_and_name[0],
                )
            )
    if not resource_requests:
        raise exceptions.NoSharedResourcesError(
            message=f"{english_lang_code} and {random_non_english_lang_code} have either no shared books or no shared resource types or both"
        )
    return model.DocumentRequest(
        email_address=email_address,
        assembly_strategy_kind=model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
        assembly_layout_kind=assembly_layout_kind,
        layout_for_print=layout_for_print,
        generate_pdf=generate_pdf,
        generate_epub=generate_epub,
        generate_docx=generate_docx,
        resource_requests=resource_requests,
    )


@pytest.fixture()
def random_two_non_english_languages_document_request(
    email_address: pydantic.EmailStr,
    assembly_layout_kind: model.AssemblyLayoutEnum,
    layout_for_print: bool,
    generate_pdf: bool,
    generate_epub: bool,
    generate_docx: bool,
    random_non_english_lang_code: str,
    random_non_english_lang_code2: str,
) -> model.DocumentRequest:
    """
    Build one document request having resource requests for two
    non-English languages.
    """
    book_codes_and_names = resource_lookup.book_codes_for_lang(
        random_non_english_lang_code
    )
    if not book_codes_and_names:
        raise exceptions.NoBooksError(
            message=f"{random_non_english_lang_code} has no available books"
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
    # Language 2
    book_codes_and_names2 = resource_lookup.book_codes_for_lang(
        random_non_english_lang_code2
    )
    if not book_codes_and_names:
        raise exceptions.NoBooksError(
            message=f"{random_non_english_lang_code2} has no available books"
        )
    # book_code_and_name2 = random.choice(book_codes_and_names2)
    resource_types_and_names2 = resource_lookup.resource_types(
        random_non_english_lang_code2,
        ",".join(
            [book_code_and_name[0] for book_code_and_name in book_codes_and_names2]
        ),
    )
    for resource_type_and_name2 in resource_types_and_names2:
        if (
            resource_type_and_name2[1] in [r1[1] for r1 in resource_types_and_names]
            or (
                resource_type_and_name2[1] in USFM_RESOURCE_TYPES
                and any(
                    r[1] in USFM_RESOURCE_TYPES
                    for r in resource_types_and_names
                )
            )
        ) and book_code_and_name in book_codes_and_names2:
            resource_requests.append(
                model.ResourceRequest(
                    lang_code=random_non_english_lang_code2,
                    resource_type=resource_type_and_name2[0],
                    book_code=book_code_and_name[0],
                )
            )

    if not resource_requests:
        raise exceptions.NoSharedResourcesError(
            message=f"{random_non_english_lang_code} and {random_non_english_lang_code2} have no shared books, no shared resource types, or both"
        )
    return model.DocumentRequest(
        email_address=email_address,
        assembly_strategy_kind=(
            model.AssemblyStrategyEnum.INTERLEAVE_BY_CHAPTER
            if random_non_english_lang_code
            and random_non_english_lang_code2
            and random_non_english_lang_code != random_non_english_lang_code2
            and len(resource_requests) > 1
            else model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK
        ),
        assembly_layout_kind=assembly_layout_kind,
        layout_for_print=layout_for_print,
        generate_pdf=generate_pdf,
        generate_epub=generate_epub,
        generate_docx=generate_docx,
        resource_requests=resource_requests,
    )
