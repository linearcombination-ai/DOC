"""
TN book intros are shown only if both the document request asks for them
(a hashed document request field) and the deployment's SHOW_TN_BOOK_INTRO
setting allows them. These tests pin that "env AND request" behavior.
"""

from typing import Iterator, Sequence

import pytest
from doc.config import get_settings
from doc.domain import parsing
from doc.domain.model import (
    EN_TN_CONDENSED_RESOURCE_TYPE,
    TN_RESOURCE_TYPE,
    LangDirEnum,
    ResourceLookupDto,
    ResourceRequest,
    TNBook,
    TNCBook,
)


@pytest.fixture(autouse=True)
def fresh_settings() -> Iterator[None]:
    get_settings.cache_clear()
    yield
    get_settings.cache_clear()


def set_env_flag(monkeypatch: pytest.MonkeyPatch, value: bool) -> None:
    monkeypatch.setenv("SHOW_TN_BOOK_INTRO", "true" if value else "false")
    get_settings.cache_clear()


@pytest.mark.parametrize(
    "env_flag, requested, expected",
    [
        (True, True, True),
        (True, False, False),
        (False, True, False),
        (False, False, False),
    ],
)
def test_effective_flag_is_env_and_request(
    monkeypatch: pytest.MonkeyPatch,
    env_flag: bool,
    requested: bool,
    expected: bool,
) -> None:
    set_env_flag(monkeypatch, env_flag)
    assert parsing.effective_show_tn_book_intro(requested) is expected


def lookup_dto(resource_type: str) -> ResourceLookupDto:
    return ResourceLookupDto(
        lang_code="en",
        lang_name="English",
        localized_lang_name="English",
        resource_type=resource_type,
        resource_type_name="Name",
        book_code="mat",
        lang_direction=LangDirEnum.LTR,
        url=None,
    )


@pytest.mark.parametrize(
    "env_flag, requested, expected",
    [
        (True, True, True),
        (True, False, False),
        (False, True, False),
        (False, False, False),
    ],
)
def test_books_passes_the_effective_flag_to_tn_and_tnc_parsing(
    monkeypatch: pytest.MonkeyPatch,
    env_flag: bool,
    requested: bool,
    expected: bool,
) -> None:
    set_env_flag(monkeypatch, env_flag)
    received: list[tuple[str, bool]] = []

    def fake_tn_book_content(
        resource_lookup_dto: ResourceLookupDto,
        resource_dir: str,
        resource_requests: Sequence[ResourceRequest],
        layout_for_print: bool,
        show_tn_book_intro: bool,
    ) -> TNBook:
        received.append(("tn", show_tn_book_intro))
        return TNBook("en", "English", "mat", "TN", "", {}, LangDirEnum.LTR)

    def fake_tnc_book_content(
        resource_lookup_dto: ResourceLookupDto,
        resource_dir: str,
        resource_requests: Sequence[ResourceRequest],
        layout_for_print: bool,
        show_tn_book_intro: bool,
    ) -> TNCBook:
        received.append(("tnc", show_tn_book_intro))
        return TNCBook("en", "English", "mat", "TNC", "", {}, LangDirEnum.LTR)

    monkeypatch.setattr(parsing, "tn_book_content", fake_tn_book_content)
    monkeypatch.setattr(parsing, "tnc_book_content", fake_tnc_book_content)
    parsing.books(
        [
            lookup_dto(TN_RESOURCE_TYPE),
            lookup_dto(EN_TN_CONDENSED_RESOURCE_TYPE),
        ],
        ["unused_dir_1", "unused_dir_2"],
        [],
        False,
        False,
        False,
        requested,
    )
    assert received == [("tn", expected), ("tnc", expected)]
