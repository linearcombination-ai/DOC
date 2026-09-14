import re

import pytest

from doc.domain import document_generator, model


def test_document_request_key_too_long_for_semantic_result() -> None:
    """
    Use enough resource requests that a semantic name built from them
    will be too long which will cause the document request key algorithm
    to use a timestamp-based, non-semantic, name.
    """
    components = [
        (
            "bdf",
            "reg",
            "mat",
        ),
        (
            "bdf",
            "reg",
            "mrk",
        ),
        (
            "pt-br",
            "ulb",
            "mat",
        ),
        (
            "pt-br",
            "tw",
            "mat",
        ),
        (
            "pt-br",
            "tq",
            "mat",
        ),
        (
            "pt-br",
            "tn",
            "mat",
        ),
        (
            "pt-br",
            "ulb",
            "mrk",
        ),
        (
            "pt-br",
            "tw",
            "mrk",
        ),
        (
            "pt-br",
            "tq",
            "mrk",
        ),
        (
            "pt-br",
            "tn",
            "mrk",
        ),
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
    ]
    resource_requests = [
        model.ResourceRequest(
            lang_code=component[0],
            resource_type=component[1],
            book_code=component[2],
        )
        for component in components
    ]
    assembly_strategy_kind = model.AssemblyStrategyEnum.INTERLEAVE_BY_CHAPTER
    assembly_layout_kind = model.AssemblyLayoutEnum.ONE_COLUMN
    limit_words = True
    use_chapter_labels = True
    use_section_visual_separator = True
    key = document_generator.document_request_key(
        resource_requests,
        assembly_strategy_kind,
        assembly_layout_kind,
        limit_words,
        use_chapter_labels,
        use_section_visual_separator,
        use_two_column_layout_for_tn_notes=False,
        use_two_column_layout_for_tq_notes=True,
        show_tn_book_intro=True,
        show_bc_book_intro=True,
        show_tn_chapter_intro=True,
        show_bc_chapter_commentary=True,
        show_rg_chapter_commentary=True,
    )
    assert re.search(r"[0-9]+_[0-9]+", key)


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

    def fake_book_codes_for_lang_from_usfm_only(lang_code: str):
        nonlocal call_count
        call_count += 1
        assert lang_code == "pt-br"
        return list(localized_names_by_book_code.items())

    monkeypatch.setattr(
        document_generator.resource_lookup,
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

    def fake_book_codes_for_lang_from_usfm_only(lang_code: str):
        calls.append(lang_code)
        return list(localized_names_by_lang[lang_code].items())

    monkeypatch.setattr(
        document_generator.resource_lookup,
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
