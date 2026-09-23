import re

from doc.config import settings
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


def test_assemble_content_for_tw_only_request_links_to_external_tw_resource() -> None:
    """
    When TW is the only resource requested (no USFM, TN, TNC, TQ, BC, or RG
    books), the per-book assembly strategies have nothing to iterate over
    since TW is language-level, not book-level. Previously this meant the
    resulting document was completely empty. Now assemble_content should
    fall back to linking to each requested language's external TW resource
    page instead of producing an empty document.
    """
    tw_book = model.TWBook(
        lang_code="en",
        lang_name="English",
        resource_type_name="Translation Words",
        lang_direction=model.LangDirEnum.LTR,
    )
    document_request = model.DocumentRequest(
        assembly_strategy_kind=model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
        resource_requests=[
            model.ResourceRequest(lang_code="en", resource_type="tw", book_code="mat"),
        ],
    )

    document_parts = document_generator.assemble_content(
        "test-key",
        document_request,
        [],  # usfm_books
        [],  # tn_books
        [],  # tnc_books
        [],  # tq_books
        [tw_book],  # tw_books
        [],  # bc_books
        [],  # rg_books
    )

    assert document_parts
    expected_link = settings.BIEL_TW_RESOURCE_URL_FMT_STR.format(
        tw_book.lang_code, tw_book.lang_name
    )
    assert any(expected_link in part.content for part in document_parts)


def test_assemble_content_for_tw_only_request_links_each_unique_language() -> None:
    """
    A TW-only request for multiple languages should produce one external
    TW resource link per unique language requested.
    """
    tw_book_en = model.TWBook(
        lang_code="en",
        lang_name="English",
        resource_type_name="Translation Words",
        lang_direction=model.LangDirEnum.LTR,
    )
    tw_book_fr = model.TWBook(
        lang_code="fr",
        lang_name="French",
        resource_type_name="Translation Words",
        lang_direction=model.LangDirEnum.LTR,
    )
    document_request = model.DocumentRequest(
        assembly_strategy_kind=model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
        resource_requests=[
            model.ResourceRequest(lang_code="en", resource_type="tw", book_code="mat"),
            model.ResourceRequest(lang_code="fr", resource_type="tw", book_code="mat"),
        ],
    )

    document_parts = document_generator.assemble_content(
        "test-key",
        document_request,
        [],
        [],
        [],
        [],
        [tw_book_en, tw_book_fr],
        [],
        [],
    )

    content = "".join(part.content for part in document_parts)
    assert (
        settings.BIEL_TW_RESOURCE_URL_FMT_STR.format(
            tw_book_en.lang_code, tw_book_en.lang_name
        )
        in content
    )
    assert (
        settings.BIEL_TW_RESOURCE_URL_FMT_STR.format(
            tw_book_fr.lang_code, tw_book_fr.lang_name
        )
        in content
    )


def test_assemble_content_for_usfm_and_tw_request_is_unaffected() -> None:
    """
    The TW-only fallback must not change behavior when USFM (or other
    book-level resources) are requested alongside TW: the per-book
    assembly strategy should run as before and no external TW link
    fallback should be appended.
    """
    usfm_book = model.USFMBook(
        lang_code="en",
        lang_name="English",
        localized_lang_name="English",
        book_code="mat",
        national_book_name="Matthew",
        resource_type_name="Unlocked Literal Bible",
        chapters={},
        lang_direction=model.LangDirEnum.LTR,
    )
    tw_book = model.TWBook(
        lang_code="en",
        lang_name="English",
        resource_type_name="Translation Words",
        lang_direction=model.LangDirEnum.LTR,
    )
    document_request = model.DocumentRequest(
        assembly_strategy_kind=model.AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
        resource_requests=[
            model.ResourceRequest(lang_code="en", resource_type="ulb", book_code="mat"),
            model.ResourceRequest(lang_code="en", resource_type="tw", book_code="mat"),
        ],
    )

    document_parts = document_generator.assemble_content(
        "test-key",
        document_request,
        [usfm_book],
        [],
        [],
        [],
        [tw_book],
        [],
        [],
    )

    expected_link = settings.BIEL_TW_RESOURCE_URL_FMT_STR.format(
        tw_book.lang_code, tw_book.lang_name
    )
    content = "".join(part.content for part in document_parts)
    assert expected_link not in content
