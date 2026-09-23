from doc.domain import model
from doc.domain.assembly_strategies.assemble_by_book import (
    assemble_content_by_verse_book_at_a_time,
)


def _usfm_book(lang_code: str, book_code: str) -> model.USFMBook:
    return model.USFMBook(
        lang_code=lang_code,
        lang_name=lang_code,
        localized_lang_name=lang_code,
        book_code=book_code,
        national_book_name=book_code.upper(),
        resource_type_name="ULB",
        chapters={
            1: model.USFMChapter(
                content=(
                    '<span class="verse">'
                    '<sup class="versemarker">1</sup>'
                    f"{book_code} verse text"
                    "</span>"
                ),
                verses=None,
            )
        },
        lang_direction=model.LangDirEnum.LTR,
    )


def _tn_book(lang_code: str, book_code: str) -> model.TNBook:
    return model.TNBook(
        lang_code=lang_code,
        lang_name=lang_code,
        book_code=book_code,
        resource_type_name="Translation Notes",
        book_intro=f"Intro for {book_code}",
        chapters={
            1: model.TNChapter(
                intro_html=f"<p>{book_code} chapter intro</p>",
                verses={"1": f"<p>{book_code} note 1</p>"},
            )
        },
        lang_direction=model.LangDirEnum.LTR,
    )


def test_assemble_content_by_verse_book_at_a_time_does_not_duplicate_books_when_one_book_lacks_usfm() -> (
    None
):
    """
    Regression test for a bug where assemble_content_by_verse_book_at_a_time's
    fallback branch -- taken whenever a (lang_code, book_code) pair has no
    matching USFM resource, the routine case of a gateway-language
    notes-only book requested alongside fully-translated books -- called
    assemble_content_by_book with the entire, unfiltered book lists instead
    of assembling just that one book. That reprocessed every book in the
    request from scratch, duplicating already-assembled book intros (and
    other content) for the books that DID have USFM.
    """
    usfm_books = [_usfm_book("en", "gen"), _usfm_book("en", "exo")]
    tn_books = [
        _tn_book("en", "gen"),
        _tn_book("en", "exo"),
        # "lev" has TN but no USFM translation yet -- a normal, common case.
        _tn_book("en", "lev"),
    ]

    document_parts = assemble_content_by_verse_book_at_a_time(
        usfm_books,
        tn_books,
        [],
        [],
        [],
        [],
        [],
        assembly_layout_kind=model.AssemblyLayoutEnum.ONE_COLUMN,
        use_section_visual_separator=False,
        use_two_column_layout_for_tn_notes=False,
        use_two_column_layout_for_tq_notes=False,
        show_tn_book_intro=True,
        show_bc_book_intro=True,
        show_tn_chapter_intro=True,
        show_bc_chapter_commentary=True,
        show_rg_chapter_commentary=True,
    )

    def count_parts_containing(needle: str) -> int:
        return sum(1 for part in document_parts if needle in (part.content or ""))

    # Each book's TN intro and USFM chapter content must appear exactly
    # once, not once per (lang_code, book_code) pair lacking USFM.
    assert count_parts_containing("Intro for gen") == 1
    assert count_parts_containing("Intro for exo") == 1
    assert count_parts_containing("Intro for lev") == 1
    assert count_parts_containing("gen verse text") == 1
    assert count_parts_containing("exo verse text") == 1
