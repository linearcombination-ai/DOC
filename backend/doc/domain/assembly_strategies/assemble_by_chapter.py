from itertools import zip_longest
from typing import Mapping, Sequence

from doc.config import settings
from doc.domain.assembly_strategies.assembly_strategy_utils import (
    chapter_commentary,
    chapter_commentary_parts,
    collect_unique_book_codes,
    collect_unique_lang_codes,
    filter_books_by_book_code,
    filter_books_by_lang_code,
    get_book_intros,
    get_book_intros_for_groups,
    get_chapter_intros,
    get_chapter_intros_for_groups,
    get_non_usfm_resources_chapter,
    get_usfm_and_tw,
    get_usfm_and_tw_verse,
    order_usfm_resources,
    tn_chapter_intro,
    tnc_chapter_intro,
    tn_verses_parts,
    tnc_verses_parts,
    tq_verses_parts,
)
from doc.utils.text_utils import demote_headings_by_one
from doc.domain.bible_books import BOOK_CHAPTERS
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


def assemble_content_by_chapter(
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
    book_chapters: Mapping[str, int] = BOOK_CHAPTERS,
) -> list[DocumentPart]:
    document_parts: list[DocumentPart] = []
    book_codes = collect_unique_book_codes(
        usfm_books,
        tn_books,
        tnc_books,
        tq_books,
        tw_books,
        bc_books,
    )
    for book_code in book_codes:
        (
            selected_usfm_books,
            selected_tn_books,
            selected_tnc_books,
            selected_tq_books,
            selected_bc_books,
        ) = filter_books_by_book_code(
            usfm_books, tn_books, tnc_books, tq_books, bc_books, book_code
        )
        if selected_usfm_books:
            document_parts.extend(
                assemble_usfm_by_chapter(
                    book_chapters[book_code],
                    selected_usfm_books,
                    selected_tn_books,
                    selected_tnc_books,
                    selected_tq_books,
                    tw_books,
                    selected_bc_books,
                    use_section_visual_separator,
                    use_two_column_layout_for_tn_notes,
                    use_two_column_layout_for_tq_notes,
                    show_tn_book_intro,
                    show_bc_book_intro,
                    show_tn_chapter_intro,
                )
            )
        elif not selected_usfm_books and (selected_tn_books or selected_tnc_books):
            document_parts.extend(
                assemble_tn_by_chapter(
                    book_chapters[book_code],
                    selected_usfm_books,
                    selected_tn_books,
                    selected_tnc_books,
                    selected_tq_books,
                    tw_books,
                    selected_bc_books,
                    use_section_visual_separator,
                    use_two_column_layout_for_tn_notes,
                    use_two_column_layout_for_tq_notes,
                    show_tn_book_intro,
                    show_bc_book_intro,
                    show_tn_chapter_intro,
                )
            )
        elif (
            not selected_usfm_books
            and not selected_tn_books
            and not selected_tnc_books
            and selected_tq_books
        ):
            document_parts.extend(
                assemble_tq_by_chapter(
                    book_chapters[book_code],
                    selected_usfm_books,
                    selected_tn_books,
                    selected_tnc_books,
                    selected_tq_books,
                    tw_books,
                    selected_bc_books,
                    use_section_visual_separator,
                    use_two_column_layout_for_tq_notes,
                    show_tn_book_intro,
                    show_bc_book_intro,
                    show_tn_chapter_intro,
                )
            )
        elif (
            not selected_usfm_books
            and not selected_tn_books
            and not selected_tnc_books
            and not selected_tq_books
            and (tw_books or selected_bc_books)
        ):
            document_parts.extend(
                assemble_tw_by_chapter(
                    selected_usfm_books,
                    selected_tn_books,
                    selected_tnc_books,
                    selected_tq_books,
                    tw_books,
                    selected_bc_books,
                    use_section_visual_separator,
                    show_bc_book_intro,
                )
            )
    return document_parts


def assemble_content_by_verse_chapter_at_a_time(
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
    book_chapters: Mapping[str, int] = BOOK_CHAPTERS,
) -> list[DocumentPart]:
    document_parts: list[DocumentPart] = []
    if not usfm_books:  # usfm not provided so versification doesn't apply
        document_parts.extend(
            assemble_content_by_chapter(
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
    elif usfm_books and (
        assembly_layout_kind == AssemblyLayoutEnum.ONE_COLUMN
        or assembly_layout_kind == AssemblyLayoutEnum.ONE_COLUMN_COMPACT
    ):
        book_codes = collect_unique_book_codes(
            usfm_books,
            tn_books,
            tnc_books,
            tq_books,
            tw_books,
            bc_books,
        )
        for book_code in book_codes:
            (
                selected_usfm_books,
                selected_tn_books,
                selected_tnc_books,
                selected_tq_books,
                selected_bc_books,
            ) = filter_books_by_book_code(
                usfm_books,
                tn_books,
                tnc_books,
                tq_books,
                bc_books,
                book_code,
            )
            if selected_usfm_books and (
                assembly_layout_kind == AssemblyLayoutEnum.ONE_COLUMN
                or assembly_layout_kind == AssemblyLayoutEnum.ONE_COLUMN_COMPACT
            ):
                document_parts.extend(
                    assemble_usfm_by_verse_chapter_at_a_time(
                        book_chapters[book_code],
                        selected_usfm_books,
                        selected_tn_books,
                        selected_tnc_books,
                        selected_tq_books,
                        tw_books,
                        selected_bc_books,
                        use_section_visual_separator,
                        use_two_column_layout_for_tn_notes,
                        use_two_column_layout_for_tq_notes,
                        show_tn_book_intro,
                        show_bc_book_intro,
                        show_tn_chapter_intro,
                        show_bc_chapter_commentary,
                    )
                )
    return document_parts


def assemble_usfm_by_chapter(
    num_chapters: int,
    usfm_books: Sequence[USFMBook],
    tn_books: Sequence[TNBook],
    tnc_books: Sequence[TNCBook],
    tq_books: Sequence[TQBook],
    tw_books: Sequence[TWBook],
    bc_books: Sequence[BCBook],
    use_section_visual_separator: bool,
    use_two_column_layout_for_tn_notes: bool,
    use_two_column_layout_for_tq_notes: bool,
    show_tn_book_intro: bool,
    show_bc_book_intro: bool,
    show_tn_chapter_intro: bool,
    fmt_str: str = settings.BOOK_NAME_FMT_STR,
) -> list[DocumentPart]:
    is_rtl = usfm_books[0].lang_direction == LangDirEnum.RTL if usfm_books else False
    document_parts: list[DocumentPart] = []
    book_intros = get_book_intros_for_groups(
        tn_books,
        tnc_books,
        bc_books,
        is_rtl,
        show_tn_book_intro,
        show_bc_book_intro,
        use_section_visual_separator,
    )
    document_parts.extend(book_intros)
    lang_codes = collect_unique_lang_codes(
        usfm_books, tn_books, tnc_books, tq_books, tw_books, bc_books
    )
    for chapter_num in range(1, num_chapters + 1):
        for lang_code in lang_codes:
            tw_book = None
            selected_tw_books = [
                tw_book for tw_book in tw_books if tw_book.lang_code == lang_code
            ]
            if selected_tw_books:
                tw_book = selected_tw_books[0]
            selected_usfm_books = [
                usfm_book
                for usfm_book in usfm_books
                if usfm_book.lang_code == lang_code
            ]
            usfm_book = None
            usfm_book2 = None
            if len(selected_usfm_books) == 1:
                usfm_book = selected_usfm_books[0]
            elif len(selected_usfm_books) == 2:
                usfm_book, usfm_book2 = order_usfm_resources(selected_usfm_books)
            if usfm_book:
                document_parts.append(
                    DocumentPart(
                        content=fmt_str.format(usfm_book.national_book_name),
                        is_rtl=is_rtl,
                    )
                )
                if chapter_num in usfm_book.chapters:
                    usfm_and_tw = get_usfm_and_tw(
                        usfm_book.resource_type_name,
                        usfm_book.chapters[chapter_num].content,
                        tw_book,
                        is_rtl,
                        use_section_visual_separator,
                    )
                    document_parts.extend(usfm_and_tw)
            chapter_intros = get_chapter_intros_for_groups(
                tn_books,
                tnc_books,
                bc_books,
                chapter_num,
                lang_code,
                is_rtl,
                show_tn_chapter_intro,
                use_section_visual_separator,
            )
            document_parts.extend(chapter_intros)
            for bc_book in [
                bc_book for bc_book in bc_books if bc_book.lang_code == lang_code
            ]:
                document_parts.extend(
                    chapter_commentary_parts(
                        bc_book, chapter_num, is_rtl, use_section_visual_separator
                    )
                )
            for tn_book in [
                tn_book for tn_book in tn_books if tn_book.lang_code == lang_code
            ]:
                document_parts.extend(
                    tn_verses_parts(
                        tn_book,
                        chapter_num,
                        is_rtl,
                        use_two_column_layout_for_tn_notes,
                        use_section_visual_separator,
                    )
                )
            for tnc_book in [
                tnc_book for tnc_book in tnc_books if tnc_book.lang_code == lang_code
            ]:
                document_parts.extend(
                    tnc_verses_parts(
                        tnc_book,
                        chapter_num,
                        is_rtl,
                        use_two_column_layout_for_tn_notes,
                        use_section_visual_separator,
                    )
                )
            for tq_book in [
                tq_book for tq_book in tq_books if tq_book.lang_code == lang_code
            ]:
                document_parts.extend(
                    tq_verses_parts(
                        tq_book,
                        chapter_num,
                        is_rtl,
                        use_two_column_layout_for_tn_notes,
                        use_section_visual_separator,
                    )
                )
            if usfm_book2:
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


def assemble_usfm_by_verse_chapter_at_a_time(
    num_chapters: int,
    usfm_books: Sequence[USFMBook],
    tn_books: Sequence[TNBook],
    tnc_books: Sequence[TNCBook],
    tq_books: Sequence[TQBook],
    tw_books: Sequence[TWBook],
    bc_books: Sequence[BCBook],
    use_section_visual_separator: bool,
    use_two_column_layout_for_tn_notes: bool,
    use_two_column_layout_for_tq_notes: bool,
    show_tn_book_intro: bool,
    show_bc_book_intro: bool,
    show_tn_chapter_intro: bool,
    show_bc_chapter_commentary: bool,
    fmt_str: str = settings.RESOURCE_TYPE_NAME_FMT_STR,
) -> list[DocumentPart]:
    is_rtl = usfm_books[0].lang_direction == LangDirEnum.RTL if usfm_books else False
    document_parts: list[DocumentPart] = []
    if show_tn_book_intro:
        for tn_book in tn_books:
            if tn_book.book_intro:
                document_parts.append(
                    DocumentPart(
                        content=fmt_str.format(tn_book.resource_type_name),
                        is_rtl=is_rtl,
                    )
                )
                document_parts.append(
                    DocumentPart(
                        content=demote_headings_by_one(tn_book.book_intro),
                        is_rtl=is_rtl,
                        use_section_visual_separator=use_section_visual_separator,
                    )
                )
        for tnc_book in tnc_books:
            if tnc_book.book_intro:
                document_parts.append(
                    DocumentPart(
                        content=fmt_str.format(tnc_book.resource_type_name),
                        is_rtl=is_rtl,
                    )
                )
                document_parts.append(
                    DocumentPart(
                        content=demote_headings_by_one(tnc_book.book_intro),
                        is_rtl=is_rtl,
                        use_section_visual_separator=use_section_visual_separator,
                    )
                )
    if show_bc_book_intro:
        for bc_book in bc_books:
            if bc_book.book_intro:
                document_parts.append(
                    DocumentPart(
                        content=fmt_str.format(bc_book.resource_type_name),
                        is_rtl=is_rtl,
                    )
                )
                document_parts.append(
                    DocumentPart(
                        content=demote_headings_by_one(bc_book.book_intro),
                        is_rtl=is_rtl,
                        use_section_visual_separator=use_section_visual_separator,
                    )
                )
    lang_codes = collect_unique_lang_codes(
        usfm_books, tn_books, tnc_books, tq_books, tw_books, bc_books
    )
    for chapter_num in range(1, num_chapters + 1):
        for lang_code in lang_codes:
            (
                selected_usfm_books,
                selected_tn_books,
                selected_tnc_books,
                selected_tq_books,
                selected_tw_books,
                selected_bc_books,
            ) = filter_books_by_lang_code(
                usfm_books,
                tn_books,
                tnc_books,
                tq_books,
                tw_books,
                bc_books,
                lang_code,
            )
            tw_book = selected_tw_books[0] if selected_tw_books else None
            usfm_book = None
            usfm_book2 = None
            usfm_chapter = None
            usfm_book2_chapter = None
            if len(selected_usfm_books) == 1:
                usfm_book = selected_usfm_books[0]
            elif len(selected_usfm_books) == 2:
                usfm_book, usfm_book2 = order_usfm_resources(selected_usfm_books)
            if usfm_book and chapter_num in usfm_book.chapters:
                usfm_chapter = usfm_book.chapters[chapter_num]
            if usfm_book2 and chapter_num in usfm_book2.chapters:
                usfm_book2_chapter = usfm_book2.chapters[chapter_num]
            tn_chapter = (
                selected_tn_books[0].chapters.get(chapter_num)
                if selected_tn_books
                else None
            )
            tnc_chapter = (
                selected_tnc_books[0].chapters.get(chapter_num)
                if selected_tnc_books
                else None
            )
            tq_chapter = (
                selected_tq_books[0].chapters.get(chapter_num)
                if selected_tq_books
                else None
            )
            if show_bc_chapter_commentary:
                for bc_book in selected_bc_books:
                    commentary = chapter_commentary(bc_book, chapter_num)
                    if commentary:
                        document_parts.append(
                            DocumentPart(
                                content=fmt_str.format(bc_book.resource_type_name),
                                is_rtl=is_rtl,
                            )
                        )
                        document_parts.append(
                            DocumentPart(
                                content=commentary,
                                is_rtl=is_rtl,
                                use_section_visual_separator=use_section_visual_separator,
                            )
                        )
            if show_tn_chapter_intro:
                for tn_book in selected_tn_books:
                    intro = tn_chapter_intro(tn_book, chapter_num)
                    if intro:
                        document_parts.append(
                            DocumentPart(
                                content=fmt_str.format(tn_book.resource_type_name),
                                is_rtl=is_rtl,
                            )
                        )
                        document_parts.append(
                            DocumentPart(
                                content=intro,
                                is_rtl=is_rtl,
                                use_section_visual_separator=use_section_visual_separator,
                            )
                        )
                for tnc_book in selected_tnc_books:
                    intro = tnc_chapter_intro(
                        tnc_book,
                        chapter_num,
                        use_section_visual_separator,
                    )
                    if intro:
                        document_parts.append(
                            DocumentPart(
                                content=fmt_str.format(tnc_book.resource_type_name),
                                is_rtl=is_rtl,
                            )
                        )
                        document_parts.append(
                            DocumentPart(
                                content=intro,
                                is_rtl=is_rtl,
                                use_section_visual_separator=use_section_visual_separator,
                            )
                        )
            primary_verses = None
            secondary_verses = None
            if usfm_book and usfm_chapter:
                primary_verses = usfm_chapter.verses
            if usfm_book2 and usfm_book2_chapter:
                secondary_verses = usfm_book2_chapter.verses
            if usfm_book and primary_verses:
                for verse_ref, verse in primary_verses.items():
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
                    if (
                        tn_chapter
                        and tn_chapter.verses
                        and verse_ref in tn_chapter.verses
                    ):
                        document_parts.append(
                            DocumentPart(
                                content=fmt_str.format(
                                    selected_tn_books[0].resource_type_name
                                ),
                                is_rtl=is_rtl,
                            )
                        )
                        document_parts.append(
                            DocumentPart(
                                content=tn_chapter.verses[verse_ref],
                                is_rtl=is_rtl,
                                use_section_visual_separator=use_section_visual_separator,
                            )
                        )
                    if (
                        tnc_chapter
                        and tnc_chapter.verses
                        and verse_ref in tnc_chapter.verses
                    ):
                        document_parts.append(
                            DocumentPart(
                                content=fmt_str.format(
                                    selected_tnc_books[0].resource_type_name
                                ),
                                is_rtl=is_rtl,
                            )
                        )
                        document_parts.append(
                            DocumentPart(
                                content=tnc_chapter.verses[verse_ref],
                                is_rtl=is_rtl,
                                use_section_visual_separator=use_section_visual_separator,
                            )
                        )
                    if (
                        tq_chapter
                        and tq_chapter.verses
                        and verse_ref in tq_chapter.verses
                    ):
                        document_parts.append(
                            DocumentPart(
                                content=fmt_str.format(
                                    selected_tq_books[0].resource_type_name
                                ),
                                is_rtl=is_rtl,
                            )
                        )
                        document_parts.append(
                            DocumentPart(
                                content=tq_chapter.verses[verse_ref],
                                is_rtl=is_rtl,
                                use_section_visual_separator=use_section_visual_separator,
                            )
                        )
                    if (
                        usfm_book2
                        and secondary_verses
                        and verse_ref in secondary_verses
                    ):
                        document_parts.extend(
                            get_usfm_and_tw_verse(
                                usfm_book2.national_book_name,
                                chapter_num,
                                verse_ref,
                                usfm_book2.resource_type_name,
                                secondary_verses[verse_ref],
                                tw_book,
                                is_rtl,
                                use_section_visual_separator,
                            )
                        )
    return document_parts


def assemble_tn_by_chapter(
    num_chapters: int,
    usfm_books: Sequence[USFMBook],
    tn_books: Sequence[TNBook],
    tnc_books: Sequence[TNCBook],
    tq_books: Sequence[TQBook],
    tw_books: Sequence[TWBook],
    bc_books: Sequence[BCBook],
    use_section_visual_separator: bool,
    use_two_column_layout_for_tn_notes: bool,
    use_two_column_layout_for_tq_notes: bool,
    show_tn_book_intro: bool,
    show_bc_book_intro: bool,
    show_tn_chapter_intro: bool,
    # fmt_str: str = settings.LEFT_ALIGNED_HEADER_FMT_STR,
) -> list[DocumentPart]:
    tn_is_rtl = tn_books[0].lang_direction == LangDirEnum.RTL if tn_books else False
    tnc_is_rtl = tnc_books[0].lang_direction == LangDirEnum.RTL if tnc_books else False
    is_rtl = tn_is_rtl or tnc_is_rtl
    document_parts: list[DocumentPart] = []
    document_parts.extend(
        get_book_intros_for_groups(
            tn_books,
            tnc_books,
            bc_books,
            is_rtl,
            show_tn_book_intro,
            show_bc_book_intro,
            use_section_visual_separator,
        )
    )
    lang_codes = collect_unique_lang_codes(
        usfm_books, tn_books, tnc_books, tq_books, tw_books, bc_books
    )
    for chapter_num in range(1, num_chapters + 1):
        for lang_code in lang_codes:
            document_parts.extend(
                get_chapter_intros_for_groups(
                    [tn_book for tn_book in tn_books if tn_book.lang_code == lang_code],
                    [
                        tnc_book
                        for tnc_book in tnc_books
                        if tnc_book.lang_code == lang_code
                    ],
                    [bc_book for bc_book in bc_books if bc_book.lang_code == lang_code],
                    chapter_num,
                    lang_code,
                    is_rtl,
                    show_tn_chapter_intro,
                    use_section_visual_separator,
                )
            )
            for tn_book in [
                tn_book for tn_book in tn_books if tn_book.lang_code == lang_code
            ]:
                document_parts.extend(
                    tn_verses_parts(
                        tn_book,
                        chapter_num,
                        is_rtl,
                        use_two_column_layout_for_tn_notes,
                        use_section_visual_separator,
                    )
                )
            for tnc_book in [
                tnc_book for tnc_book in tnc_books if tnc_book.lang_code == lang_code
            ]:
                document_parts.extend(
                    tnc_verses_parts(
                        tnc_book,
                        chapter_num,
                        is_rtl,
                        use_two_column_layout_for_tn_notes,
                        use_section_visual_separator,
                    )
                )
            for tq_book in [
                tq_book for tq_book in tq_books if tq_book.lang_code == lang_code
            ]:
                document_parts.extend(
                    tq_verses_parts(
                        tq_book,
                        chapter_num,
                        is_rtl,
                        use_two_column_layout_for_tq_notes,
                        use_section_visual_separator,
                    )
                )
    return document_parts


def assemble_tq_by_chapter(
    num_chapters: int,
    usfm_books: Sequence[USFMBook],
    tn_books: Sequence[TNBook],
    tnc_books: Sequence[TNCBook],
    tq_books: Sequence[TQBook],
    tw_books: Sequence[TWBook],
    bc_books: Sequence[BCBook],
    use_section_visual_separator: bool,
    use_two_column_layout_for_tq_notes: bool,
    show_tn_book_intro: bool,
    show_bc_book_intro: bool,
    show_tn_chapter_intro: bool,
) -> list[DocumentPart]:
    is_rtl = tq_books[0].lang_direction == LangDirEnum.RTL if tq_books else False
    document_parts: list[DocumentPart] = []
    lang_codes = collect_unique_lang_codes(
        usfm_books, tn_books, tnc_books, tq_books, tw_books, bc_books
    )
    for chapter_num in range(1, num_chapters + 1):
        for lang_code in lang_codes:
            document_parts.append(
                DocumentPart(
                    content=f"Chapter {chapter_num}",
                    is_rtl=is_rtl,
                )
            )
            for bc_book in [
                bc_book for bc_book in bc_books if bc_book.lang_code == lang_code
            ]:
                document_parts.extend(
                    chapter_commentary_parts(
                        bc_book, chapter_num, is_rtl, use_section_visual_separator
                    )
                )
            for tq_book in [
                tq_book for tq_book in tq_books if tq_book.lang_code == lang_code
            ]:
                document_parts.extend(
                    tq_verses_parts(
                        tq_book,
                        chapter_num,
                        is_rtl,
                        use_two_column_layout_for_tq_notes,
                        use_section_visual_separator,
                    )
                )
    return document_parts


def assemble_tw_by_chapter(
    usfm_books: Sequence[USFMBook],
    tn_books: Sequence[TNBook],
    tnc_books: Sequence[TNCBook],
    tq_books: Sequence[TQBook],
    tw_books: Sequence[TWBook],
    bc_books: Sequence[BCBook],
    use_section_visual_separator: bool,
    show_bc_book_intro: bool,
) -> list[DocumentPart]:
    is_rtl = tw_books[0].lang_direction == LangDirEnum.RTL if tw_books else False
    document_parts: list[DocumentPart] = []
    for bc_book in bc_books:
        book_intros = get_book_intros(
            None,
            None,
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
                None,
                None,
                bc_book,
                chapter_num,
                is_rtl,
                False,
                use_section_visual_separator,
            )
            document_parts.extend(chapter_intros)
            non_usfm_resources = get_non_usfm_resources_chapter(
                None,
                None,
                None,
                bc_book,
                chapter_num,
                is_rtl,
                False,
                False,
                use_section_visual_separator,
            )
            document_parts.extend(non_usfm_resources)
    return document_parts
