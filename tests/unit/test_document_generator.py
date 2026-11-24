import re
from typing import Annotated, Mapping

import pytest
from doc.domain import document_generator, model, resource_lookup
from doc.domain.bible_books import BOOK_NAMES

KEY_PATTERN = re.compile(r"^[a-z0-9_-]+_[0-9a-f]{12}$")


def document_request(
    components: list[tuple[str, str, str]] | None = None,
    overrides: Mapping[str, object] | None = None,
) -> model.DocumentRequest:
    components = components or [
        ("en", "ulb", "mat"),
        ("en", "tn", "mat"),
        ("en", "tw", "mat"),
    ]
    request = model.DocumentRequest(
        assembly_strategy_kind=model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
        assembly_layout_kind=model.AssemblyLayoutEnum.ONE_COLUMN,
        resource_requests=[
            model.ResourceRequest(
                lang_code=lang_code, resource_type=resource_type, book_code=book_code
            )
            for lang_code, resource_type, book_code in components
        ],
    )
    return request.model_copy(update=dict(overrides or {}))


def test_every_document_request_field_carries_exactly_one_marker() -> None:
    """
    A new DocumentRequest field must be deliberately classified as
    hashed into the document request key or ignored by it.
    """
    for name, field in model.DocumentRequest.model_fields.items():
        markers = [
            marker
            for marker in field.metadata
            if isinstance(marker, (model.Hashed, model.Ignored))
        ]
        assert len(markers) == 1, name
    hashed = model.DocumentRequest.hashed_fields()
    assert hashed <= set(model.DocumentRequest.model_fields)
    assert len(hashed) == 15


def test_unclassified_field_is_rejected_when_the_model_is_defined() -> None:
    with pytest.raises(TypeError, match="unclassified"):

        class Unclassified(model.CacheKeyedModel):
            marked: Annotated[int, model.Hashed()]
            unclassified: int

    with pytest.raises(TypeError, match="both"):

        class Doubly(model.CacheKeyedModel):
            both: Annotated[int, model.Hashed(), model.Ignored()]


def test_document_request_key_slug_encodes_resources_not_settings() -> None:
    key = document_generator.document_request_key(document_request())
    assert KEY_PATTERN.match(key)
    assert key.startswith("en_ulb-tn-tw_mat_")


def test_document_request_key_is_deterministic() -> None:
    assert document_generator.document_request_key(
        document_request()
    ) == document_generator.document_request_key(document_request())


def test_document_request_key_differs_for_each_hashed_setting() -> None:
    base_key = document_generator.document_request_key(document_request())
    variations: dict[str, object] = {
        "assembly_strategy_kind": model.AssemblyStrategyEnum.INTERLEAVE_BY_CHAPTER,
        "assembly_layout_kind": model.AssemblyLayoutEnum.ONE_COLUMN_COMPACT,
        "layout_for_print": True,
        "limit_words": False,
        "use_chapter_labels": True,
        "use_prince": True,
        "use_two_column_layout_for_tn_notes": True,
        "use_two_column_layout_for_tq_notes": True,
        "use_section_visual_separator": True,
        "show_tn_book_intro": False,
        "show_bc_book_intro": False,
        "show_tn_chapter_intro": False,
        "show_bc_chapter_commentary": False,
        "generate_docx": True,
    }
    assert set(variations) == set(
        model.DocumentRequest.hashed_fields() - {"resource_requests"}
    )
    keys = {base_key}
    for field, value in variations.items():
        key = document_generator.document_request_key(
            document_request(overrides={field: value})
        )
        # Same resources, so same slug, but a different hash.
        assert key.rsplit("_", 1)[0] == base_key.rsplit("_", 1)[0], field
        assert key not in keys, field
        keys.add(key)


def test_document_request_key_ignores_delivery_settings() -> None:
    base_key = document_generator.document_request_key(document_request())
    for overrides in (
        {"email_address": "someone@example.com"},
        {"generate_pdf": False},
        {"document_request_source": model.DocumentRequestSourceEnum.UI},
    ):
        assert (
            document_generator.document_request_key(
                document_request(overrides=overrides)
            )
            == base_key
        ), overrides


def test_document_request_key_limit_words_only_matters_with_tw() -> None:
    without_tw = [("en", "ulb", "mat"), ("en", "tn", "mat")]
    assert document_generator.document_request_key(
        document_request(without_tw, {"limit_words": True})
    ) == document_generator.document_request_key(
        document_request(without_tw, {"limit_words": False})
    )
    assert document_generator.document_request_key(
        document_request(overrides={"limit_words": True})
    ) != document_generator.document_request_key(
        document_request(overrides={"limit_words": False})
    )


def test_document_request_key_depends_on_resources_and_their_order() -> None:
    first = [("en", "ulb", "mat"), ("fr", "ulb", "mat")]
    second = [("fr", "ulb", "mat"), ("en", "ulb", "mat")]
    assert document_generator.document_request_key(
        document_request(first)
    ) != document_generator.document_request_key(document_request(second))
    assert document_generator.document_request_key(
        document_request(first)
    ) != document_generator.document_request_key(
        document_request([("en", "ulb", "mrk"), ("fr", "ulb", "mrk")])
    )


def test_document_request_key_is_short_and_deterministic_for_many_resources() -> None:
    """
    Use enough resource requests that the readable slug has to be
    summarized. The key must still be short, filename safe, and stable
    (previously a timestamp, and so never a cache hit, was used).
    """
    components = [
        ("bdf", "reg", "mat"),
        ("bdf", "reg", "mrk"),
        ("pt-br", "ulb", "mat"),
        ("pt-br", "tw", "mat"),
        ("pt-br", "tq", "mat"),
        ("pt-br", "tn", "mat"),
        ("pt-br", "ulb", "mrk"),
        ("pt-br", "tw", "mrk"),
        ("pt-br", "tq", "mrk"),
        ("pt-br", "tn", "mrk"),
        ("fr", "ulb", "mat"),
        ("fr", "tw", "mat"),
        ("fr", "tq", "mat"),
        ("fr", "tn", "mat"),
        ("fr", "f10", "mat"),
        ("fr", "ulb", "mrk"),
        ("fr", "tw", "mrk"),
        ("fr", "tq", "mrk"),
        ("fr", "tn", "mrk"),
        ("fr", "f10", "mrk"),
    ] + [("fr", "ulb", book_code) for book_code in list(BOOK_NAMES)[:30]]
    key = document_generator.document_request_key(document_request(components))
    assert KEY_PATTERN.match(key)
    assert len(key) < 100
    assert "books" in key
    assert key == document_generator.document_request_key(document_request(components))


def test_document_request_key_is_filename_safe_for_odd_lang_codes() -> None:
    key = document_generator.document_request_key(
        document_request([("../../Etc/passwd", "ulb", "mat")])
    )
    assert KEY_PATTERN.match(key)
    assert "/" not in key and ".." not in key


def test_document_request_key_golden_values() -> None:
    """
    Pin exact keys: changing which fields are hashed, their names, or
    how they are serialized changes every key (and so every cached file
    name), so that must be a deliberate decision.
    """

    def resource_request(
        lang_code: str, resource_type: str, book_code: str
    ) -> model.ResourceRequest:
        return model.ResourceRequest(
            lang_code=lang_code, resource_type=resource_type, book_code=book_code
        )

    assert (
        document_generator.document_request_key(
            model.DocumentRequest(
                assembly_strategy_kind=model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                resource_requests=[resource_request("en", "ulb", "mat")],
            )
        )
        == "en_ulb_mat_70c7420bb621"
    )
    assert (
        document_generator.document_request_key(
            model.DocumentRequest(
                assembly_strategy_kind=model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
                assembly_layout_kind=model.AssemblyLayoutEnum.ONE_COLUMN,
                document_request_source=model.DocumentRequestSourceEnum.UI,
                resource_requests=[
                    resource_request("en", "ulb", "mat"),
                    resource_request("en", "tw", "mat"),
                ],
                limit_words=False,
            )
        )
        == "en_ulb-tw_mat_50944a0aa352"
    )


def test_generate_docx_is_part_of_the_key() -> None:
    """A docx request renders unstyled HTML so it must not share a cache key."""
    assert document_generator.document_request_key(
        document_request(overrides={"generate_docx": True})
    ) != document_generator.document_request_key(document_request())


def _tn_book(
    book_code: str, english_name: str, lang_code: str = "pt-br"
) -> model.TNBook:
    return model.TNBook(
        lang_code=lang_code,
        lang_name="Brazilian Portuguese",
        book_code=book_code,
        resource_type_name="Translation Notes",
        book_intro=f"Welcome to the book of {english_name}.",
        chapters={
            1: model.TNChapter(
                intro_html=f"Introduction to chapter 1 of {english_name}.",
                verses={"1": f"A note about {english_name} 1:1."},
            )
        },
        lang_direction=model.LangDirEnum.LTR,
    )


def test_localize_non_usfm_book_names_looks_up_usfm_names_once_per_language(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """
    resource_lookup.book_codes_for_lang_from_usfm_only(lang_code) re-reads
    and re-parses every USFM file for that language's Bible-text repo, so
    for a request spanning many TN books in the same language it must be
    called once per language, not once per book (previously it was called
    once per book: an O(books requested x usfm books) cost instead of the
    O(usfm books) the work actually needs).
    """
    localized_names_by_book_code = {
        "gen": "Gênesis",
        "exo": "Êxodo",
        "lev": "Levítico",
    }
    call_count = 0

    def fake_book_codes_for_lang_from_usfm_only(
        lang_code: str,
    ) -> list[tuple[str, str]]:
        nonlocal call_count
        call_count += 1
        assert lang_code == "pt-br"
        return list(localized_names_by_book_code.items())

    monkeypatch.setattr(
        resource_lookup,
        "book_codes_for_lang_from_usfm_only",
        fake_book_codes_for_lang_from_usfm_only,
    )

    english_names_by_book_code = {"gen": "Genesis", "exo": "Exodus", "lev": "Leviticus"}
    tn_books = [
        _tn_book(book_code, english_name)
        for book_code, english_name in english_names_by_book_code.items()
    ]

    document_generator.localize_non_usfm_book_names(
        usfm_books=[],
        tn_books=tn_books,
        tnc_books=[],
        tq_books=[],
    )

    # Bounded, non-book-count-scaling: exactly one call for 3 books in one
    # language, not 3.
    assert call_count == 1

    # No behavior change: every book's English name is still replaced by
    # its localized (national) name found via the (now-cached) USFM lookup.
    for book_code, english_name in english_names_by_book_code.items():
        tn_book = next(book for book in tn_books if book.book_code == book_code)
        localized_name = localized_names_by_book_code[book_code]
        assert english_name not in tn_book.book_intro
        assert localized_name in tn_book.book_intro
        assert english_name not in tn_book.chapters[1].intro_html
        assert localized_name in tn_book.chapters[1].intro_html
        assert english_name not in tn_book.chapters[1].verses["1"]
        assert localized_name in tn_book.chapters[1].verses["1"]


def test_localize_non_usfm_book_names_calls_usfm_lookup_once_per_distinct_language(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """
    The per-language cache must key on lang_code, not collapse to a single
    global call: two different languages must each get their own lookup
    call (still bounded - once per language, not once per book).
    """
    localized_names_by_lang: dict[str, dict[str, str]] = {
        "pt-br": {"gen": "Gênesis", "exo": "Êxodo"},
        "fr": {"gen": "Genèse", "exo": "Exode"},
    }
    calls: list[str] = []

    def fake_book_codes_for_lang_from_usfm_only(
        lang_code: str,
    ) -> list[tuple[str, str]]:
        calls.append(lang_code)
        return list(localized_names_by_lang[lang_code].items())

    monkeypatch.setattr(
        resource_lookup,
        "book_codes_for_lang_from_usfm_only",
        fake_book_codes_for_lang_from_usfm_only,
    )

    tn_books = [
        _tn_book("gen", "Genesis", lang_code="pt-br"),
        _tn_book("exo", "Exodus", lang_code="pt-br"),
        _tn_book("gen", "Genesis", lang_code="fr"),
        _tn_book("exo", "Exodus", lang_code="fr"),
    ]

    document_generator.localize_non_usfm_book_names(
        usfm_books=[],
        tn_books=tn_books,
        tnc_books=[],
        tq_books=[],
    )

    # One call per distinct language (2 languages, 4 books total) - bounded
    # by language count, not book count.
    assert sorted(calls) == ["fr", "pt-br"]

    pt_br_gen = next(
        book
        for book in tn_books
        if book.lang_code == "pt-br" and book.book_code == "gen"
    )
    fr_gen = next(
        book for book in tn_books if book.lang_code == "fr" and book.book_code == "gen"
    )
    assert "Gênesis" in pt_br_gen.book_intro
    assert "Genèse" in fr_gen.book_intro
