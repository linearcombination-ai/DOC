from typing import Optional, Sequence

from doc.config import settings
from doc.domain.assembly_strategies.assembly_strategy_utils import (
    collect_unique_book_codes,
    collect_unique_lang_codes,
    get_book_intros,
    get_chapter_intros,
    get_non_usfm_resources_chapter,
    get_usfm_and_tw,
    get_usfm_and_tw_verse,
    get_non_usfm_resources_verse,
    order_usfm_resources,
)
from doc.domain.model import (
    AssemblyLayoutEnum,
    BCBook,
    DocumentPart,
    LangDirEnum,
    TNBook,
    TNCBook,
    TQBook,
    TWBook,
    USFMBook,
)

logger = settings.logger(__name__)


def assemble_content_by_book(
    usfm_books: Sequence[USFMBook],
    tn_books: Sequence[TNBook],
    tnc_books: Sequence[TNCBook],
    tq_books: Sequence[TQBook],
    tw_books: Sequence[TWBook],
    bc_books: Sequence[BCBook],
    assembly_layout_kind: AssemblyLayoutEnum,
    use_section_visual_separator: bool,
    use_two_column_layout_for_tn_notes: bool,
    use_two_column_layout_for_tq_notes: bool,
    show_tn_book_intro: bool,
    show_bc_book_intro: bool,
    show_tn_chapter_intro: bool,
) -> list[DocumentPart]:
    document_parts: list[DocumentPart] = []
    lang_codes = collect_unique_lang_codes(
        usfm_books, tn_books, tnc_books, tq_books, tw_books, bc_books
    )
    book_codes = collect_unique_book_codes(
        usfm_books, tn_books, tnc_books, tq_books, tw_books, bc_books
    )
    for lang_code in lang_codes:
        for book_code in book_codes:
            selected_usfm_books = [
                usfm_book
                for usfm_book in usfm_books
                if usfm_book.lang_code == lang_code and usfm_book.book_code == book_code
            ]
            usfm_book = None
            usfm_book2 = None
            if len(selected_usfm_books) == 1:
                usfm_book = selected_usfm_books[0]
            elif len(selected_usfm_books) == 2:
                usfm_book, usfm_book2 = order_usfm_resources(selected_usfm_books)
            tn_book = next(
                (
                    tn_book
                    for tn_book in tn_books
                    if tn_book.lang_code == lang_code and tn_book.book_code == book_code
                ),
                None,
            )
            tnc_book = next(
                (
                    tnc_book
                    for tnc_book in tnc_books
                    if tnc_book.lang_code == lang_code
                    and tnc_book.book_code == book_code
                ),
                None,
            )
            tq_book = next(
                (
                    tq_book
                    for tq_book in tq_books
                    if tq_book.lang_code == lang_code and tq_book.book_code == book_code
                ),
                None,
            )
            tw_book = next(
                (tw_book for tw_book in tw_books if tw_book.lang_code == lang_code),
                None,
            )
            bc_book = next(
                (
                    bc_book
                    for bc_book in bc_books
                    if bc_book.lang_code == lang_code and bc_book.book_code == book_code
                ),
                None,
            )
            if usfm_book is not None:
                document_parts.extend(
                    assemble_usfm_by_book(
                        usfm_book,
                        tn_book,
                        tnc_book,
                        tq_book,
                        tw_book,
                        usfm_book2,
                        bc_book,
                        use_section_visual_separator,
                        use_two_column_layout_for_tn_notes,
                        use_two_column_layout_for_tq_notes,
                        show_tn_book_intro,
                        show_bc_book_intro,
                        show_tn_chapter_intro,
                    )
                )
            elif usfm_book is None and (tn_book is not None or tnc_book is not None):
                document_parts.extend(
                    assemble_tn_by_book(
                        usfm_book,
                        tn_book,
                        tnc_book,
                        tq_book,
                        tw_book,
                        usfm_book2,
                        bc_book,
                        use_section_visual_separator,
                        use_two_column_layout_for_tn_notes,
                        use_two_column_layout_for_tq_notes,
                        show_tn_book_intro,
                        show_bc_book_intro,
                        show_tn_chapter_intro,
                    )
                )
            elif usfm_book is None and tn_book is None and tq_book is not None:
                document_parts.extend(
                    assemble_tq_by_book(
                        usfm_book,
                        tn_book,
                        tnc_book,
                        tq_book,
                        tw_book,
                        usfm_book2,
                        bc_book,
                        use_section_visual_separator,
                        use_two_column_layout_for_tq_notes,
                        show_bc_book_intro,
                    )
                )
            elif (
                usfm_book is None
                and tn_book is None
                and tq_book is None
                and (tw_book is not None or bc_book is not None)
            ):
                document_parts.extend(
                    assemble_tw_by_book(
                        usfm_book,
                        tn_book,
                        tnc_book,
                        tq_book,
                        tw_book,
                        usfm_book2,
                        bc_book,
                        use_section_visual_separator,
                        show_bc_book_intro,
                    )
                )
    return document_parts


def assemble_content_by_verse_book_at_a_time(
    usfm_books: Sequence[USFMBook],
    tn_books: Sequence[TNBook],
    tnc_books: Sequence[TNCBook],
    tq_books: Sequence[TQBook],
    tw_books: Sequence[TWBook],
    bc_books: Sequence[BCBook],
    assembly_layout_kind: AssemblyLayoutEnum,
    use_section_visual_separator: bool,
    use_two_column_layout_for_tn_notes: bool,
    use_two_column_layout_for_tq_notes: bool,
    show_tn_book_intro: bool,
    show_bc_book_intro: bool,
    show_tn_chapter_intro: bool,
    show_bc_chapter_commentary: bool,
) -> list[DocumentPart]:
    document_parts: list[DocumentPart] = []
    lang_codes = collect_unique_lang_codes(
        usfm_books, tn_books, tnc_books, tq_books, tw_books, bc_books
    )
    book_codes = collect_unique_book_codes(
        usfm_books, tn_books, tnc_books, tq_books, tw_books, bc_books
    )
    for lang_code in lang_codes:
        for book_code in book_codes:
            selected_usfm_books = [
                usfm_book
                for usfm_book in usfm_books
                if usfm_book.lang_code == lang_code and usfm_book.book_code == book_code
            ]
            usfm_book = None
            usfm_book2 = None
            if len(selected_usfm_books) == 1:
                usfm_book = selected_usfm_books[0]
            elif len(selected_usfm_books) == 2:
                usfm_book, usfm_book2 = order_usfm_resources(selected_usfm_books)
            tn_book = next(
                (
                    tn_book
                    for tn_book in tn_books
                    if tn_book.lang_code == lang_code and tn_book.book_code == book_code
                ),
                None,
            )
            tnc_book = next(
                (
                    tnc_book
                    for tnc_book in tnc_books
                    if tnc_book.lang_code == lang_code
                    and tnc_book.book_code == book_code
                ),
                None,
            )
            tq_book = next(
                (
                    tq_book
                    for tq_book in tq_books
                    if tq_book.lang_code == lang_code and tq_book.book_code == book_code
                ),
                None,
            )
            tw_book = next(
                (tw_book for tw_book in tw_books if tw_book.lang_code == lang_code),
                None,
            )
            bc_book = next(
                (
                    bc_book
                    for bc_book in bc_books
                    if bc_book.lang_code == lang_code and bc_book.book_code == book_code
                ),
                None,
            )
            if usfm_book:
                document_parts.extend(
                    assemble_usfm_by_verse_book_at_a_time(
                        usfm_book,
                        tn_book,
                        tnc_book,
                        tq_book,
                        tw_book,
                        usfm_book2,
                        bc_book,
                        use_section_visual_separator,
                        use_two_column_layout_for_tn_notes,
                        use_two_column_layout_for_tq_notes,
                        show_tn_book_intro,
                        show_bc_book_intro,
                        show_tn_chapter_intro,
                        show_bc_chapter_commentary,
                    )
                )
            else:
                document_parts.extend(
                    assemble_content_by_book(
                        usfm_books,
                        tn_books,
                        tnc_books,
                        tq_books,
                        tw_books,
                        bc_books,
                        assembly_layout_kind,
                        use_section_visual_separator,
                        use_two_column_layout_for_tn_notes,
                        use_two_column_layout_for_tq_notes,
                        show_tn_book_intro,
                        show_bc_book_intro,
                        show_tn_chapter_intro,
                    )
                )
    return document_parts


def assemble_usfm_by_book(
    usfm_book: Optional[USFMBook],
    tn_book: Optional[TNBook],
    tnc_book: Optional[TNCBook],
    tq_book: Optional[TQBook],
    tw_book: Optional[TWBook],
    usfm_book2: Optional[USFMBook],
    bc_book: Optional[BCBook],
    use_section_visual_separator: bool,
    use_two_column_layout_for_tn_notes: bool,
    use_two_column_layout_for_tq_notes: bool,
    show_tn_book_intro: bool,
    show_bc_book_intro: bool,
    show_tn_chapter_intro: bool,
    resource_type_name_fmt_str: str = settings.RESOURCE_TYPE_NAME_FMT_STR,
) -> list[DocumentPart]:
    is_rtl = usfm_book.lang_direction == LangDirEnum.RTL if usfm_book else False
    document_parts: list[DocumentPart] = []
    book_intros = get_book_intros(
        tn_book,
        tnc_book,
        bc_book,
        is_rtl,
        show_tn_book_intro,
        show_bc_book_intro,
        use_section_visual_separator,
    )
    document_parts.extend(book_intros)
    if usfm_book:
        if not book_intros:  # the book intros already give the book name
            document_parts.append(
                DocumentPart(
                    content=resource_type_name_fmt_str.format(
                        usfm_book.national_book_name
                    ),
                    is_rtl=is_rtl,
                )
            )
        for (
            chapter_num,
            chapter,
        ) in usfm_book.chapters.items():
            document_parts.extend(
                get_chapter_intros(
                    tn_book,
                    tnc_book,
                    bc_book,
                    chapter_num,
                    is_rtl,
                    show_tn_chapter_intro,
                    use_section_visual_separator,
                )
            )
            document_parts.extend(
                get_usfm_and_tw(
                    usfm_book.resource_type_name,
                    chapter.content,
                    tw_book,
                    is_rtl,
                    use_section_visual_separator,
                )
            )
            document_parts.extend(
                get_non_usfm_resources_chapter(
                    tn_book,
                    tnc_book,
                    tq_book,
                    bc_book,
                    chapter_num,
                    is_rtl,
                    use_two_column_layout_for_tn_notes,
                    use_two_column_layout_for_tq_notes,
                    use_section_visual_separator,
                )
            )
            if usfm_book2 and chapter_num in usfm_book2.chapters:
                document_parts.extend(
                    get_usfm_and_tw(
                        usfm_book2.resource_type_name,
                        usfm_book2.chapters[chapter_num].content,
                        tw_book,
                        is_rtl,
                        use_section_visual_separator,
                    )
                )
    return document_parts


def assemble_usfm_by_verse_book_at_a_time(
    usfm_book: Optional[USFMBook],
    tn_book: Optional[TNBook],
    tnc_book: Optional[TNCBook],
    tq_book: Optional[TQBook],
    tw_book: Optional[TWBook],
    usfm_book2: Optional[USFMBook],
    bc_book: Optional[BCBook],
    use_section_visual_separator: bool,
    use_two_column_layout_for_tn_notes: bool,
    use_two_column_layout_for_tq_notes: bool,
    show_tn_book_intro: bool,
    show_bc_book_intro: bool,
    show_tn_chapter_intro: bool,
    show_bc_chapter_commentary: bool,
) -> list[DocumentPart]:
    is_rtl = usfm_book.lang_direction == LangDirEnum.RTL if usfm_book else False
    document_parts: list[DocumentPart] = []
    book_intros = get_book_intros(
        tn_book,
        tnc_book,
        bc_book,
        is_rtl,
        show_tn_book_intro,
        show_bc_book_intro,
        use_section_visual_separator,
    )
    document_parts.extend(book_intros)
    if usfm_book:
        for (
            chapter_num,
            chapter,
        ) in usfm_book.chapters.items():
            chapter_intros = get_chapter_intros(
                tn_book,
                tnc_book,
                bc_book,
                chapter_num,
                is_rtl,
                show_tn_chapter_intro,
                use_section_visual_separator,
            )
            document_parts.extend(chapter_intros)
            if chapter.verses:
                for verse_ref, verse in chapter.verses.items():
                    document_parts.extend(
                        get_usfm_and_tw_verse(
                            usfm_book.national_book_name,
                            chapter_num,
                            verse_ref,
                            usfm_book.resource_type_name,
                            verse,
                            tw_book,
                            is_rtl,
                            use_section_visual_separator,
                        )
                    )
                    document_parts.extend(
                        get_non_usfm_resources_verse(
                            tn_book,
                            tnc_book,
                            tq_book,
                            bc_book,
                            verse_ref,
                            chapter_num,
                            is_rtl,
                            use_section_visual_separator,
                        )
                    )
                    # If the user chose two USFM resource types for a language. e.g., fr:
                    # ulb, f10, show the second USFM content here
                    if usfm_book2:
                        usfm_book2_chapter = usfm_book2.chapters[chapter_num]
                        if (
                            usfm_book2_chapter.verses
                            and verse_ref in usfm_book2_chapter.verses
                        ):
                            document_parts.extend(
                                get_usfm_and_tw_verse(
                                    usfm_book2.national_book_name,
                                    chapter_num,
                                    verse_ref,
                                    usfm_book2.resource_type_name,
                                    usfm_book2_chapter.verses[verse_ref],
                                    tw_book,
                                    is_rtl,
                                    use_section_visual_separator,
                                )
                            )
    return document_parts


def assemble_tn_by_book(
    usfm_book: Optional[USFMBook],
    tn_book: Optional[TNBook],
    tnc_book: Optional[TNCBook],
    tq_book: Optional[TQBook],
    tw_book: Optional[TWBook],
    usfm_book2: Optional[USFMBook],
    bc_book: Optional[BCBook],
    use_section_visual_separator: bool,
    use_two_column_layout_for_tn_notes: bool,
    use_two_column_layout_for_tq_notes: bool,
    show_tn_book_intro: bool,
    show_bc_book_intro: bool,
    show_tn_chapter_intro: bool,
) -> list[DocumentPart]:
    document_parts: list[DocumentPart] = []
    if tn_book:
        is_rtl = tn_book.lang_direction == LangDirEnum.RTL if tn_book else False
        book_intros = get_book_intros(
            tn_book,
            tnc_book,
            bc_book,
            is_rtl,
            show_tn_book_intro,
            show_bc_book_intro,
            use_section_visual_separator,
        )
        document_parts.extend(book_intros)
        for chapter_num in tn_book.chapters:
            chapter_intros = get_chapter_intros(
                tn_book,
                tnc_book,
                bc_book,
                chapter_num,
                is_rtl,
                show_tn_chapter_intro,
                use_section_visual_separator,
            )
            document_parts.extend(chapter_intros)
            chapter_non_usfm = get_non_usfm_resources_chapter(
                tn_book,
                tnc_book,
                tq_book,
                bc_book,
                chapter_num,
                is_rtl,
                use_two_column_layout_for_tn_notes,
                use_two_column_layout_for_tq_notes,
                use_section_visual_separator,
            )
            document_parts.extend(chapter_non_usfm)
    return document_parts


def assemble_tq_by_book(
    usfm_book: Optional[USFMBook],
    tn_book: Optional[TNBook],
    tnc_book: Optional[TNCBook],
    tq_book: Optional[TQBook],
    tw_book: Optional[TWBook],
    usfm_book2: Optional[USFMBook],
    bc_book: Optional[BCBook],
    use_section_visual_separator: bool,
    use_two_column_layout_for_tq_notes: bool,
    show_bc_book_intro: bool,
) -> list[DocumentPart]:
    document_parts: list[DocumentPart] = []
    if tq_book:
        is_rtl = tq_book.lang_direction == LangDirEnum.RTL
        book_intros = get_book_intros(
            tn_book,
            tnc_book,
            bc_book,
            is_rtl,
            False,
            show_bc_book_intro,
            use_section_visual_separator,
        )
        document_parts.extend(book_intros)
        for chapter_num in tq_book.chapters:
            chapter_intros = get_chapter_intros(
                tn_book,
                tnc_book,
                bc_book,
                chapter_num,
                is_rtl,
                False,
                use_section_visual_separator,
            )
            document_parts.extend(chapter_intros)
            non_usfm_resources = get_non_usfm_resources_chapter(
                tn_book,
                tnc_book,
                tq_book,
                bc_book,
                chapter_num,
                is_rtl,
                False,
                use_two_column_layout_for_tq_notes,
                use_section_visual_separator,
            )
            document_parts.extend(non_usfm_resources)
    return document_parts


def assemble_tw_by_book(
    usfm_book: Optional[USFMBook],
    tn_book: Optional[TNBook],
    tnc_book: Optional[TNCBook],
    tq_book: Optional[TQBook],
    tw_book: Optional[TWBook],
    usfm_book2: Optional[USFMBook],
    bc_book: Optional[BCBook],
    use_section_visual_separator: bool,
    show_bc_book_intro: bool,
) -> list[DocumentPart]:
    is_rtl = tw_book.lang_direction == LangDirEnum.RTL if tw_book else False
    document_parts: list[DocumentPart] = []
    book_intros = get_book_intros(
        tn_book,
        tnc_book,
        bc_book,
        is_rtl,
        False,
        show_bc_book_intro,
        use_section_visual_separator,
    )
    document_parts.extend(book_intros)
    chapters = bc_book.chapters if bc_book else []
    for chapter_num in chapters:
        chapter_intros = get_chapter_intros(
            tn_book,
            tnc_book,
            bc_book,
            chapter_num,
            is_rtl,
            False,
            use_section_visual_separator,
        )
        document_parts.extend(chapter_intros)
        non_usfm_resources = get_non_usfm_resources_chapter(
            tn_book,
            tnc_book,
            tq_book,
            bc_book,
            chapter_num,
            is_rtl,
            False,
            False,
            use_section_visual_separator,
        )
        document_parts.extend(non_usfm_resources)
    return document_parts
