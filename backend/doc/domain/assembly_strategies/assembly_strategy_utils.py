"""
Utility functions used by assembly_strategies.
"""

from re import search
from typing import Mapping, Optional, Sequence

from doc.config import settings
from doc.domain.bible_books import BOOK_ID_MAP
from doc.domain.model import (
    BCBook,
    DocumentPart,
    TNBook,
    TNCBook,
    TQBook,
    USFMBook,
    TWBook,
)
from doc.utils.tw_utils import translation_words_content
from doc.utils.text_utils import demote_headings_by_one
from docx.document import Document as DocxDocument
from docx.enum.section import WD_SECTION
from docx.enum.text import WD_BREAK
from docx.oxml import parse_xml
from docx.oxml.ns import qn

logger = settings.logger(__name__)


OXML_LANGUAGE_LIST: list[str] = [
    "ar-SA",
    "bg-BG",
    "zh-CN",
    "zh-TW",
    "hr-HR",
    "cs-CZ",
    "da-DK",
    "nl-NL",
    "en-US",
    "et-EE",
    "fi-FI",
    "fr-FR",
    "de-DE",
    "el-GR",
    "he-IL",
    "hi-IN",
    "hu-HU",
    "id-ID",
    "it-IT",
    "ja-JP",
    "kk-KZ",
    "ko-KR",
    "lv-LV",
    "lt-LT",
    "ms-MY",
    "nb-NO",
    "pl-PL",
    "pt-BR",
    "pt-PT",
    "ro-RO",
    "ru-RU",
    "sr-latn-RS",
    "sk-SK",
    "sl-SI",
    "es-ES",
    "sv-SE",
    "th-TH",
    "tr-TR",
    "uk-UA",
    "vi-VN",
]
OXML_LANGUAGE_LIST_LOWERCASE: list[str] = [
    language.lower() for language in OXML_LANGUAGE_LIST
]
OXML_LANGUAGE_LIST_LOWERCASE_SPLIT: list[str] = [
    language for language in OXML_LANGUAGE_LIST_LOWERCASE if "-" in language
]


def chapter_commentary(
    bc_book: Optional[BCBook],
    chapter_num: int,
) -> str:
    """Get the chapter commentary."""
    content = ""
    if (
        bc_book
        and chapter_num in bc_book.chapters
        and bc_book.chapters[chapter_num].commentary
    ):
        content = bc_book.chapters[chapter_num].commentary
    return content


def tn_chapter_intro(
    tn_book: Optional[TNBook],
    chapter_num: int,
) -> str:
    """Get the chapter intro."""
    content = []
    if (
        tn_book
        and chapter_num in tn_book.chapters
        and tn_book.chapters[chapter_num].intro_html
    ):
        content.append(demote_headings_by_one(tn_book.chapters[chapter_num].intro_html))
    return "".join(content)


def tn_chapter_verses(
    tn_book: Optional[TNBook],
    chapter_num: int,
    use_two_column_layout_for_tn_notes: bool,
) -> str:
    """
    Return the HTML for verses that are in the chapter with
    chapter_num.
    """
    tn_verse_notes_enclosing_div_fmt_str: str = (
        "<div style='column-count: 2;'>{}</div>"
        if use_two_column_layout_for_tn_notes
        else "<div>{}</div>"
    )
    content = []
    if tn_book and chapter_num in tn_book.chapters:
        tn_verses = tn_book.chapters[chapter_num].verses
        content.append(
            tn_verse_notes_enclosing_div_fmt_str.format("".join(tn_verses.values()))
        )
    return "".join(content)


def tq_chapter_verses(
    tq_book: Optional[TQBook],
    chapter_num: int,
    use_two_column_layout_for_tq_notes: bool,
) -> str:
    """Return the HTML for verses in chapter_num."""
    tq_verse_notes_enclosing_div_fmt_str: str = (
        "<div style='column-count: 2;'>{}</div>"
        if use_two_column_layout_for_tq_notes
        else "<div>{}</div>"
    )
    content = []
    if tq_book and chapter_num in tq_book.chapters:
        tq_verses = tq_book.chapters[chapter_num].verses
        content.append(
            tq_verse_notes_enclosing_div_fmt_str.format("".join(tq_verses.values()))
        )
    return "".join(content)


def add_full_width_hr(doc: DocxDocument) -> None:
    """Add a full-width horizontal rule that spans the entire page width."""
    p = doc.add_paragraph()
    run = p.add_run()
    # Adjust this width to your page layout; 6.5" = 8.5" page minus 1" margins on each side
    width_inches = 6.5
    hr_xml = f"""
    <w:drawing xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
               xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
               xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
               xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing">
      <wp:inline distT="0" distB="0" distL="0" distR="0">
        <wp:extent cx="{int(width_inches * 914400)}" cy="12700"/> <!-- height = 0.5pt -->
        <wp:effectExtent l="0" t="0" r="0" b="0"/>
        <wp:docPr id="1" name="FullWidthLine"/>
        <a:graphic>
          <a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture">
            <pic:pic xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture">
              <pic:nvPicPr>
                <pic:cNvPr id="0" name="HR"/>
                <pic:cNvPicPr/>
              </pic:nvPicPr>
              <pic:blipFill>
                <a:blip/>
                <a:stretch><a:fillRect/></a:stretch>
              </pic:blipFill>
              <pic:spPr>
                <a:xfrm>
                  <a:off x="0" y="0"/>
                  <a:ext cx="{int(width_inches * 914400)}" cy="12700"/>
                </a:xfrm>
                <a:prstGeom prst="rect"><a:avLst/></a:prstGeom>
                <a:solidFill>
                  <a:srgbClr val="000000"/>
                </a:solidFill>
              </pic:spPr>
            </pic:pic>
          </a:graphicData>
        </a:graphic>
      </wp:inline>
    </w:drawing>
    """
    drawing = parse_xml(hr_xml)
    run._r.append(drawing)


def add_one_column_section(doc: DocxDocument) -> None:
    """
    Add new section having 1 column to contain next content in Docx instance.
    """
    # Get ready for one column again (this matters the 2nd to Nth times in the loop).
    new_section = doc.add_section(WD_SECTION.CONTINUOUS)
    new_section.start_type

    # Set to one column layout for subdocument to be added next.
    sectPr = new_section._sectPr
    cols = sectPr.xpath("./w:cols")[0]
    cols.set(qn("w:num"), "1")


def add_two_column_section(doc: DocxDocument) -> None:
    """
    Add new section having 2 column to contain next content in Docx instance.
    """
    # Start new section for different (two) column layout.
    new_section = doc.add_section(WD_SECTION.CONTINUOUS)
    new_section.start_type
    sectPr = new_section._sectPr
    cols = sectPr.xpath("./w:cols")[0]
    cols.set(qn("w:num"), "2")
    cols.set(qn("w:space"), "10")  # Set space between columns to 10 points ->0.01"


def add_page_break(doc: DocxDocument) -> None:
    """Add page break."""
    # Add page break at end of chapter content
    p = doc.add_paragraph("")
    run = p.add_run()
    run.add_break(WD_BREAK.PAGE)


def get_book_intros(
    tn_book: Optional[TNBook],
    tnc_book: Optional[TNCBook],
    bc_book: Optional[BCBook],
    is_rtl: bool,
    show_tn_book_intro: bool,
    show_bc_book_intro: bool,
    use_section_visual_separator: bool,
    resource_type_name_fmt_str: str = settings.RESOURCE_TYPE_NAME_FMT_STR,
) -> list[DocumentPart]:
    document_parts: list[DocumentPart] = []
    if show_tn_book_intro:
        if tn_book and tn_book.book_intro:
            document_parts.append(
                DocumentPart(
                    content=resource_type_name_fmt_str.format(
                        tn_book.resource_type_name
                    ),
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
        if tnc_book and tnc_book.book_intro:
            document_parts.append(
                DocumentPart(
                    content=resource_type_name_fmt_str.format(
                        tnc_book.resource_type_name
                    ),
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
    if show_bc_book_intro and bc_book and bc_book.book_intro:
        document_parts.append(
            DocumentPart(
                content=resource_type_name_fmt_str.format(bc_book.resource_type_name),
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
    return document_parts


def get_book_intros_for_groups(
    tn_books: Sequence[TNBook],
    tnc_books: Sequence[TNCBook],
    bc_books: Sequence[BCBook],
    is_rtl: bool,
    show_tn_book_intro: bool,
    show_bc_book_intro: bool,
    use_section_visual_separator: bool,
    resource_type_name_fmt_str: str = settings.RESOURCE_TYPE_NAME_FMT_STR,
) -> list[DocumentPart]:
    document_parts: list[DocumentPart] = []
    if show_tn_book_intro:
        for tn_book in tn_books:
            if tn_book.book_intro:
                document_parts.append(
                    DocumentPart(
                        content=resource_type_name_fmt_str.format(
                            tn_book.resource_type_name
                        ),
                        is_rtl=is_rtl,
                    )
                )
                book_intro_adj = demote_headings_by_one(tn_book.book_intro)
                document_parts.append(
                    DocumentPart(
                        content=book_intro_adj,
                        is_rtl=is_rtl,
                        use_section_visual_separator=use_section_visual_separator,
                    )
                )
        for tnc_book in tnc_books:
            if tnc_book.book_intro:
                document_parts.append(
                    DocumentPart(
                        content=resource_type_name_fmt_str.format(
                            tnc_book.resource_type_name
                        ),
                        is_rtl=is_rtl,
                    )
                )
                tnc_book_intro_adj = demote_headings_by_one(tnc_book.book_intro)
                document_parts.append(
                    DocumentPart(
                        content=tnc_book_intro_adj,
                        is_rtl=is_rtl,
                        use_section_visual_separator=use_section_visual_separator,
                    )
                )
    if show_bc_book_intro:
        for bc_book in bc_books:
            if show_bc_book_intro and bc_book.book_intro:
                document_parts.append(
                    DocumentPart(
                        content=resource_type_name_fmt_str.format(
                            bc_book.resource_type_name
                        ),
                        is_rtl=is_rtl,
                    )
                )
                document_parts.append(
                    DocumentPart(
                        content=bc_book.book_intro,
                        use_section_visual_separator=use_section_visual_separator,
                    )
                )
    return document_parts


def tn_chapter_intro_parts(
    tn_book: Optional[TNBook],
    chapter_num: int,
    is_rtl: bool,
    use_section_visual_separator: bool,
    resource_type_name_fmt_str: str = settings.RESOURCE_TYPE_NAME_FMT_STR,
) -> Sequence[DocumentPart]:
    document_parts: list[DocumentPart] = []
    tn_chapter_intro_ = tn_chapter_intro(tn_book, chapter_num)
    if tn_chapter_intro_ and tn_book:
        document_parts.append(
            DocumentPart(
                content=resource_type_name_fmt_str.format(tn_book.resource_type_name),
                is_rtl=is_rtl,
            )
        )
        document_parts.append(
            DocumentPart(
                content=tn_chapter_intro_,
                is_rtl=is_rtl,
                use_section_visual_separator=use_section_visual_separator,
            )
        )
    return document_parts


def tnc_chapter_intro_parts(
    tnc_book: Optional[TNCBook],
    chapter_num: int,
    is_rtl: bool,
    use_section_visual_separator: bool,
    resource_type_name_fmt_str: str = settings.RESOURCE_TYPE_NAME_FMT_STR,
) -> Sequence[DocumentPart]:
    document_parts: list[DocumentPart] = []
    tnc_chapter_intro_ = tnc_chapter_intro(
        tnc_book, chapter_num, use_section_visual_separator
    )
    if tnc_chapter_intro_ and tnc_book:
        document_parts.append(
            DocumentPart(
                content=resource_type_name_fmt_str.format(tnc_book.resource_type_name),
                is_rtl=is_rtl,
            )
        )
        document_parts.append(
            DocumentPart(
                content=tnc_chapter_intro_,
                is_rtl=is_rtl,
                use_section_visual_separator=use_section_visual_separator,
            )
        )
    return document_parts


def get_chapter_intros(
    tn_book: Optional[TNBook],
    tnc_book: Optional[TNCBook],
    bc_book: Optional[BCBook],
    chapter_num: int,
    is_rtl: bool,
    show_tn_chapter_intro: bool,
    use_section_visual_separator: bool,
) -> list[DocumentPart]:
    document_parts: list[DocumentPart] = []
    if show_tn_chapter_intro:
        document_parts.extend(
            tn_chapter_intro_parts(
                tn_book, chapter_num, is_rtl, use_section_visual_separator
            )
        )
        document_parts.extend(
            tnc_chapter_intro_parts(
                tnc_book, chapter_num, is_rtl, use_section_visual_separator
            )
        )
    return document_parts


def get_chapter_intros_for_groups(
    tn_books: Sequence[TNBook],
    tnc_books: Sequence[TNCBook],
    bc_books: Sequence[BCBook],
    chapter_num: int,
    lang_code: str,
    is_rtl: bool,
    show_tn_chapter_intro: bool,
    use_section_visual_separator: bool,
    resource_type_name_fmt_str: str = settings.RESOURCE_TYPE_NAME_FMT_STR,
) -> list[DocumentPart]:
    document_parts: list[DocumentPart] = []
    if show_tn_chapter_intro:
        for tn_book in [
            tn_book for tn_book in tn_books if tn_book.lang_code == lang_code
        ]:
            tn_chapter_intro_ = tn_chapter_intro(tn_book, chapter_num)
            if tn_chapter_intro_:
                document_parts.append(
                    DocumentPart(
                        content=resource_type_name_fmt_str.format(
                            tn_book.resource_type_name
                        ),
                        is_rtl=is_rtl,
                    )
                )
                document_parts.append(
                    DocumentPart(
                        content=tn_chapter_intro_,
                        is_rtl=is_rtl,
                        use_section_visual_separator=use_section_visual_separator,
                    )
                )
        for tnc_book in [
            tnc_book for tnc_book in tnc_books if tnc_book.lang_code == lang_code
        ]:
            tnc_chapter_intro_ = tnc_chapter_intro(
                tnc_book, chapter_num, use_section_visual_separator
            )
            if tnc_chapter_intro_:
                document_parts.append(
                    DocumentPart(
                        content=resource_type_name_fmt_str.format(
                            tnc_book.resource_type_name
                        ),
                        is_rtl=is_rtl,
                    )
                )
                document_parts.append(
                    DocumentPart(
                        content=tnc_chapter_intro_,
                        is_rtl=is_rtl,
                        use_section_visual_separator=use_section_visual_separator,
                    )
                )
    for bc_book in [bc_book for bc_book in bc_books if bc_book.lang_code == lang_code]:
        chapter_commentary_ = chapter_commentary(bc_book, chapter_num)
        if chapter_commentary_:
            document_parts.append(
                DocumentPart(
                    content=resource_type_name_fmt_str.format(
                        bc_book.resource_type_name
                    ),
                    is_rtl=is_rtl,
                )
            )
            document_parts.append(
                DocumentPart(
                    content=chapter_commentary_,
                    is_rtl=is_rtl,
                    use_section_visual_separator=use_section_visual_separator,
                )
            )
    return document_parts


def two_column_spanning_hr_trick(
    use_section_visual_separator: bool, html_whitespace_char: str = "&nbsp;"
) -> DocumentPart:
    """
    This is a trick to make an hr after a two column section by tricking
    the html to docx parser into keeping this part using an HTML space
    rather than an empty string.
    """
    return DocumentPart(
        content=html_whitespace_char,
        use_section_visual_separator=use_section_visual_separator,
    )


def get_usfm_and_tw(
    usfm_resource_type_name: str,
    content: str,
    tw_book: Optional[TWBook],
    is_rtl: bool,
    use_section_visual_separator: bool,
    resource_type_name_fmt_str: str = settings.RESOURCE_TYPE_NAME_FMT_STR,
) -> Sequence[DocumentPart]:
    document_parts: list[DocumentPart] = []
    document_parts.append(
        DocumentPart(
            content=resource_type_name_fmt_str.format(usfm_resource_type_name),
            is_rtl=is_rtl,
        )
    )
    document_parts.append(
        DocumentPart(
            content=content,
            is_rtl=is_rtl,
            use_section_visual_separator=use_section_visual_separator,
        )
    )
    if tw_book:
        document_parts.extend(
            translation_words_content(tw_book, content, use_section_visual_separator)
        )
    return document_parts


def get_usfm_and_tw_verse(
    national_book_name: str,
    chapter_num: int,
    verse_ref: str,
    usfm_resource_type_name: str,
    verse: str,
    tw_book: Optional[TWBook],
    is_rtl: bool,
    use_section_visual_separator: bool,
    fmt_str: str = settings.RESOURCE_TYPE_NAME_FMT_STR,
    verse_span_fmt_str: str = settings.VERSE_SPAN_FMT_STR,
) -> Sequence[DocumentPart]:
    document_parts: list[DocumentPart] = []
    document_parts.append(
        DocumentPart(
            content=fmt_str.format(f"{national_book_name} {chapter_num}:{verse_ref}"),
            is_rtl=is_rtl,
        )
    )
    document_parts.append(
        DocumentPart(
            content=fmt_str.format(usfm_resource_type_name),
            is_rtl=is_rtl,
        )
    )
    document_parts.append(
        DocumentPart(
            content=verse_span_fmt_str.format(verse),
            is_rtl=is_rtl,
            use_section_visual_separator=use_section_visual_separator,
        )
    )
    if tw_book:
        document_parts.extend(
            translation_words_content(tw_book, verse, use_section_visual_separator)
        )
    return document_parts


def chapter_commentary_parts(
    bc_book: Optional[BCBook],
    chapter_num: int,
    is_rtl: bool,
    use_section_visual_separator: bool,
    resource_type_name_fmt_str: str = settings.RESOURCE_TYPE_NAME_FMT_STR,
) -> Sequence[DocumentPart]:
    document_parts: list[DocumentPart] = []
    chapter_commentary_ = chapter_commentary(bc_book, chapter_num)
    if chapter_commentary_ and bc_book:
        document_parts.append(
            DocumentPart(
                content=resource_type_name_fmt_str.format(bc_book.resource_type_name),
                is_rtl=is_rtl,
            )
        )
        document_parts.append(
            DocumentPart(
                content=chapter_commentary_,
                is_rtl=is_rtl,
                use_section_visual_separator=use_section_visual_separator,
            )
        )
    return document_parts


def tn_verses_parts(
    tn_book: Optional[TNBook],
    chapter_num: int,
    is_rtl: bool,
    use_two_column_layout_for_tn_notes: bool,
    use_section_visual_separator: bool,
    resource_type_name_fmt_str: str = settings.RESOURCE_TYPE_NAME_FMT_STR,
) -> Sequence[DocumentPart]:
    document_parts: list[DocumentPart] = []
    tn_verses = tn_chapter_verses(
        tn_book,
        chapter_num,
        use_two_column_layout_for_tn_notes,
    )
    if tn_book and tn_verses:
        document_parts.append(
            DocumentPart(
                content=resource_type_name_fmt_str.format(tn_book.resource_type_name),
                is_rtl=is_rtl,
            )
        )
        document_parts.append(
            DocumentPart(
                content=tn_verses,
                is_rtl=is_rtl,
                contained_in_two_column_section=use_two_column_layout_for_tn_notes,
            )
        )
        document_parts.append(
            two_column_spanning_hr_trick(use_section_visual_separator)
        )
    return document_parts


def tnc_verses_parts(
    tnc_book: Optional[TNCBook],
    chapter_num: int,
    is_rtl: bool,
    use_two_column_layout_for_tn_notes: bool,
    use_section_visual_separator: bool,
    resource_type_name_fmt_str: str = settings.RESOURCE_TYPE_NAME_FMT_STR,
) -> Sequence[DocumentPart]:
    document_parts: list[DocumentPart] = []
    tnc_verses = tnc_chapter_verses(
        tnc_book,
        chapter_num,
        use_two_column_layout_for_tn_notes,
    )
    if tnc_book and tnc_verses:
        document_parts.append(
            DocumentPart(
                content=resource_type_name_fmt_str.format(tnc_book.resource_type_name),
                is_rtl=is_rtl,
            )
        )
        document_parts.append(
            DocumentPart(
                content=tnc_verses,
                is_rtl=is_rtl,
                contained_in_two_column_section=use_two_column_layout_for_tn_notes,
            )
        )
        document_parts.append(
            two_column_spanning_hr_trick(use_section_visual_separator)
        )
    return document_parts


def tq_verses_parts(
    tq_book: Optional[TQBook],
    chapter_num: int,
    is_rtl: bool,
    use_two_column_layout_for_tq_notes: bool,
    use_section_visual_separator: bool,
    resource_type_name_fmt_str: str = settings.RESOURCE_TYPE_NAME_FMT_STR,
) -> Sequence[DocumentPart]:
    document_parts: list[DocumentPart] = []
    tq_verses = tq_chapter_verses(
        tq_book,
        chapter_num,
        use_two_column_layout_for_tq_notes,
    )
    if tq_book and tq_verses:
        document_parts.append(
            DocumentPart(
                content=resource_type_name_fmt_str.format(tq_book.resource_type_name),
                is_rtl=is_rtl,
            )
        )
        document_parts.append(
            DocumentPart(
                content=tq_verses,
                is_rtl=is_rtl,
                contained_in_two_column_section=use_two_column_layout_for_tq_notes,
            )
        )
        document_parts.append(
            two_column_spanning_hr_trick(use_section_visual_separator)
        )
    return document_parts


def get_non_usfm_resources_chapter(
    tn_book: Optional[TNBook],
    tnc_book: Optional[TNCBook],
    tq_book: Optional[TQBook],
    bc_book: Optional[BCBook],
    chapter_num: int,
    is_rtl: bool,
    use_two_column_layout_for_tn_notes: bool,
    use_two_column_layout_for_tq_notes: bool,
    use_section_visual_separator: bool,
) -> list[DocumentPart]:
    document_parts: list[DocumentPart] = []
    document_parts.extend(
        chapter_commentary_parts(
            bc_book, chapter_num, is_rtl, use_section_visual_separator
        )
    )
    document_parts.extend(
        tn_verses_parts(
            tn_book,
            chapter_num,
            is_rtl,
            use_two_column_layout_for_tn_notes,
            use_section_visual_separator,
        )
    )
    document_parts.extend(
        tnc_verses_parts(
            tnc_book,
            chapter_num,
            is_rtl,
            use_two_column_layout_for_tn_notes,
            use_section_visual_separator,
        )
    )
    document_parts.extend(
        tq_verses_parts(
            tq_book,
            chapter_num,
            is_rtl,
            use_two_column_layout_for_tq_notes,
            use_section_visual_separator,
        )
    )
    # document_parts.extend(
    #     rg_verses_parts(rg_book, chapter_num, is_rtl, use_section_visual_separator)
    # )
    return document_parts


def get_non_usfm_resources_verse(
    tn_book: Optional[TNBook],
    tnc_book: Optional[TNCBook],
    tq_book: Optional[TQBook],
    bc_book: Optional[BCBook],
    verse_ref: str,
    chapter_num: int,
    is_rtl: bool,
    use_section_visual_separator: bool,
    resource_type_name_fmt_str: str = settings.RESOURCE_TYPE_NAME_FMT_STR,
) -> list[DocumentPart]:
    document_parts: list[DocumentPart] = []
    tn_chapter = tn_book.chapters[chapter_num] if tn_book else None
    tnc_chapter = tnc_book.chapters[chapter_num] if tnc_book else None
    tq_chapter = tq_book.chapters[chapter_num] if tq_book else None
    if tn_book and tn_chapter and tn_chapter.verses and verse_ref in tn_chapter.verses:
        document_parts.append(
            DocumentPart(
                content=resource_type_name_fmt_str.format(tn_book.resource_type_name),
                is_rtl=is_rtl,
                use_section_visual_separator=False,
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
        tnc_book
        and tnc_chapter
        and tnc_chapter.verses
        and verse_ref in tnc_chapter.verses
    ):
        document_parts.append(
            DocumentPart(
                content=resource_type_name_fmt_str.format(tnc_book.resource_type_name),
                is_rtl=is_rtl,
                use_section_visual_separator=False,
            )
        )
        document_parts.append(
            DocumentPart(
                content=tnc_chapter.verses[verse_ref],
                is_rtl=is_rtl,
                use_section_visual_separator=use_section_visual_separator,
            )
        )
    if tq_book and tq_chapter and tq_chapter.verses and verse_ref in tq_chapter.verses:
        document_parts.append(
            DocumentPart(
                content=resource_type_name_fmt_str.format(tq_book.resource_type_name),
                is_rtl=is_rtl,
                use_section_visual_separator=False,
            )
        )
        document_parts.append(
            DocumentPart(
                content=tq_chapter.verses[verse_ref],
                is_rtl=is_rtl,
                use_section_visual_separator=use_section_visual_separator,
            )
        )
    return document_parts


def tnc_chapter_intro(
    tnc_book: Optional[TNCBook],
    chapter_num: int,
    use_section_visual_separator: bool,
    hr: str = settings.HR,
) -> str:
    """Get the chapter intro."""
    content = []
    if (
        tnc_book
        and chapter_num in tnc_book.chapters
        and tnc_book.chapters[chapter_num].intro_html
    ):
        content.append(
            demote_headings_by_one(tnc_book.chapters[chapter_num].intro_html)
        )
        if use_section_visual_separator:
            content.append(hr)
    return "".join(content)


def tnc_chapter_verses(
    tnc_book: Optional[TNCBook],
    chapter_num: int,
    use_two_column_layout_for_tn_notes: bool,
) -> str:
    """
    Return the HTML for verses that are in the chapter with
    chapter_num.
    """
    tn_verse_notes_enclosing_div_fmt_str = (
        "<div style='column-count: 2; padding-top: 2px; padding-bottom: 4px;'>{}</div>"
        if use_two_column_layout_for_tn_notes
        else "<div>{}</div>"
    )
    content = []
    if tnc_book and chapter_num in tnc_book.chapters:
        tnc_verses = tnc_book.chapters[chapter_num].verses
        content.append(
            tn_verse_notes_enclosing_div_fmt_str.format("".join(tnc_verses.values()))
        )
    return "".join(content)


def languages_in_books(usfm_books: Sequence[USFMBook]) -> Sequence[str]:
    """
    Return the distinct languages in the usfm_books.

    """
    language_set = set()
    for book in usfm_books:
        language_set.add(book.lang_code)
    languages = sorted(language_set)
    return languages


def interleave(
    lang0_usfm_books: Sequence[USFMBook], lang1_usfm_books: Sequence[USFMBook]
) -> Sequence[USFMBook]:
    """
    Interleave USFM books and then flatten list of tuples into regular flat list
    """
    interleaved = []
    max_len = max(len(lang0_usfm_books), len(lang1_usfm_books))
    for i in range(max_len):
        if i < len(lang0_usfm_books):
            interleaved.append(lang0_usfm_books[i])
        if i < len(lang1_usfm_books):
            interleaved.append(lang1_usfm_books[i])
    return interleaved


def collect_unique_lang_codes(
    usfm_books: Sequence[USFMBook],
    tn_books: Sequence[TNBook],
    tnc_books: Sequence[TNCBook],
    tq_books: Sequence[TQBook],
    tw_books: Sequence[TWBook],
    bc_books: Sequence[BCBook],
) -> list[str]:
    """Extract unique language codes from all book sequences."""
    all_lang_codes: list[str] = []
    all_lang_codes.extend(usfm_book.lang_code for usfm_book in usfm_books)
    all_lang_codes.extend(tn_book.lang_code for tn_book in tn_books)
    all_lang_codes.extend(tnc_book.lang_code for tnc_book in tnc_books)
    all_lang_codes.extend(tq_book.lang_code for tq_book in tq_books)
    all_lang_codes.extend(tw_book.lang_code for tw_book in tw_books)
    all_lang_codes.extend(bc_book.lang_code for bc_book in bc_books)
    # Preserve order while removing duplicates
    seen: set[str] = set()
    result: list[str] = []
    for lang_code in all_lang_codes:
        if lang_code not in seen:
            seen.add(lang_code)
            result.append(lang_code)
    return result


def collect_unique_book_codes(
    usfm_books: Sequence[USFMBook],
    tn_books: Sequence[TNBook],
    tnc_books: Sequence[TNCBook],
    tq_books: Sequence[TQBook],
    tw_books: Sequence[TWBook],
    bc_books: Sequence[BCBook],
    book_id_map: dict[str, int] = BOOK_ID_MAP,
) -> list[str]:
    """Extract unique book codes from all book sequences."""
    book_codes: set[str] = set()
    book_codes.update(usfm_book.book_code for usfm_book in usfm_books)
    book_codes.update(tn_book.book_code for tn_book in tn_books)
    book_codes.update(tnc_book.book_code for tnc_book in tnc_books)
    book_codes.update(tq_book.book_code for tq_book in tq_books)
    book_codes.update(bc_book.book_code for bc_book in bc_books)
    return sorted(book_codes, key=lambda book_code: book_id_map[book_code])


def filter_books_by_book_code(
    usfm_books: Sequence[USFMBook],
    tn_books: Sequence[TNBook],
    tnc_books: Sequence[TNCBook],
    tq_books: Sequence[TQBook],
    bc_books: Sequence[BCBook],
    book_code: str,
) -> tuple[
    list[USFMBook],
    list[TNBook],
    list[TNCBook],
    list[TQBook],
    list[BCBook],
]:
    selected_usfm_books = [
        usfm_book for usfm_book in usfm_books if usfm_book.book_code == book_code
    ]
    selected_tn_books = [
        tn_book for tn_book in tn_books if tn_book.book_code == book_code
    ]
    selected_tnc_books = [
        tnc_book for tnc_book in tnc_books if tnc_book.book_code == book_code
    ]
    selected_tq_books = [
        tq_book for tq_book in tq_books if tq_book.book_code == book_code
    ]
    selected_bc_books = [
        bc_book for bc_book in bc_books if bc_book.book_code == book_code
    ]
    return (
        selected_usfm_books,
        selected_tn_books,
        selected_tnc_books,
        selected_tq_books,
        selected_bc_books,
    )


def filter_books_by_lang_code(
    usfm_books: Sequence[USFMBook],
    tn_books: Sequence[TNBook],
    tnc_books: Sequence[TNCBook],
    tq_books: Sequence[TQBook],
    tw_books: Sequence[TWBook],
    bc_books: Sequence[BCBook],
    lang_code: str,
) -> tuple[
    list[USFMBook],
    list[TNBook],
    list[TNCBook],
    list[TQBook],
    list[TWBook],
    list[BCBook],
]:
    selected_usfm_books = [
        usfm_book for usfm_book in usfm_books if usfm_book.lang_code == lang_code
    ]
    selected_tn_books = [
        tn_book for tn_book in tn_books if tn_book.lang_code == lang_code
    ]
    selected_tnc_books = [
        tnc_book for tnc_book in tnc_books if tnc_book.lang_code == lang_code
    ]
    selected_tq_books = [
        tq_book for tq_book in tq_books if tq_book.lang_code == lang_code
    ]
    selected_tw_books = [
        tw_book for tw_book in tw_books if tw_book.lang_code == lang_code
    ]
    selected_bc_books = [
        bc_book for bc_book in bc_books if bc_book.lang_code == lang_code
    ]
    return (
        selected_usfm_books,
        selected_tn_books,
        selected_tnc_books,
        selected_tq_books,
        selected_tw_books,
        selected_bc_books,
    )


def order_usfm_resources(
    usfm_books: Sequence[USFMBook],
    resource_type_codes_and_names: Mapping[
        str, str
    ] = settings.RESOURCE_TYPE_CODES_AND_NAMES,
) -> tuple[USFMBook, USFMBook]:
    usfm_book0 = usfm_books[0]
    if (
        usfm_book0.lang_code == "fr"
        and usfm_book0.resource_type_name
        in [
            resource_type_codes_and_names.get("ulb", ""),
        ]
        or usfm_book0.resource_type_name
        in [
            resource_type_codes_and_names.get("udb", ""),
        ]
        or usfm_book0.lang_code == "id"
        and usfm_book0.resource_type_name
        in [
            resource_type_codes_and_names.get("ayt", ""),
        ]
        or usfm_book0.lang_code == "pt-br"
        and usfm_book0.resource_type_name
        in [
            resource_type_codes_and_names.get("blv", ""),
        ]
    ):
        usfm_book = usfm_books[1]
        usfm_book2 = usfm_books[0]
    else:
        usfm_book = usfm_books[0]
        usfm_book2 = usfm_books[1]
    return usfm_book, usfm_book2


if __name__ == "__main__":

    # To run the doctests in this module, in the root of the project do:
    # python backend/document/domain/resource_lookup.py
    # or
    # python backend/document/domain/resource_lookup.py -v
    # See https://docs.python.org/3/library/doctest.html
    # for more details.
    import doctest

    doctest.testmod()
