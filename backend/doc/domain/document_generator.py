"""
Entrypoint for backend. Here incoming document requests are processed
and eventually a final document produced.
"""

import re
import subprocess
import time
from datetime import datetime
from os.path import exists, join
from typing import Final, Mapping, Optional, Sequence, TypeAlias, cast

from celery import current_task
from doc.config import settings
from doc.domain import parsing, resource_lookup, worker
from doc.domain.assembly_strategies.assemble_by_book import (
    assemble_content_by_book,
    assemble_content_by_verse_book_at_a_time,
)
from doc.domain.assembly_strategies.assemble_by_chapter import (
    assemble_content_by_chapter,
    assemble_content_by_verse_chapter_at_a_time,
)
from doc.domain.assembly_strategies.assembly_strategy_utils import (
    add_full_width_hr,
    add_one_column_section,
    add_page_break,
    add_two_column_section,
    two_column_spanning_hr_trick,
)
from doc.domain.bible_books import BOOK_ID_MAP, BOOK_NAMES
from doc.domain.email_utils import send_email_with_attachment, should_send_email
from doc.domain.model import (
    AssemblyLayoutEnum,
    AssemblyStrategyEnum,
    Attachment,
    BCBook,
    DocumentPart,
    DocumentRequest,
    DocumentRequestSourceEnum,
    LangDirEnum,
    ResourceLookupDto,
    ResourceRequest,
    TNBook,
    TNCBook,
    TQBook,
    TWBook,
    USFMBook,
)
from doc.reviewers_guide.model import RGBook
from doc.utils.docx_util import (
    add_internal_docx_links,
    generate_docx_toc,
    override_and_clean_hyperlinks,
    style_superscripts,
)
from doc.utils.file_utils import (
    docx_filepath,
    epub_filepath,
    file_needs_update,
    html_filepath,
    pdf_filepath,
    write_file,
)
from doc.utils.template_env import env
from doc.utils.tw_utils import (
    contains_tw,
    filter_unique_by_lang_code,
    translation_words_section_for_book,
)
from docx import Document
from docx.document import Document as DocxDocument
from docx.enum.section import WD_SECTION
from docx.shared import RGBColor
from docxcompose.composer import Composer  # type: ignore
from docxtpl import DocxTemplate  # type: ignore
from html4docx import HtmlToDocx  # type: ignore

# import regex as re # not yet supported in python 3.13 - used for unicode word boundaries for RTL languages

logger = settings.logger(__name__)


LangCode: TypeAlias = str
BookCode: TypeAlias = str
Key: TypeAlias = tuple[LangCode, BookCode]

ReplacementEntry: TypeAlias = tuple[re.Pattern[str], str]  # (pattern, replacement)
ReplacementMap: TypeAlias = dict[Key, ReplacementEntry]

BOOK_NAMES: Final[Mapping[BookCode, str]] = BOOK_NAMES


def initialize_document_request_and_key(
    document_request_json: str,
) -> tuple[DocumentRequest, str]:
    document_request = DocumentRequest.parse_raw(document_request_json)
    logger.info("document_request: %s", document_request)
    document_request.assembly_layout_kind = select_assembly_layout_kind(
        document_request
    )
    # Generate the document request key that identifies this and
    # identical document requests.
    document_request_key_ = document_request_key(
        document_request.resource_requests,
        document_request.assembly_strategy_kind,
        document_request.assembly_layout_kind,
        document_request.limit_words,
        document_request.use_chapter_labels,
        document_request.use_section_visual_separator,
        document_request.use_two_column_layout_for_tn_notes,
        document_request.use_two_column_layout_for_tq_notes,
        document_request.show_tn_book_intro,
        document_request.show_tn_chapter_intro,
        document_request.show_bc_book_intro,
        document_request.show_bc_chapter_commentary,
        document_request.show_rg_chapter_commentary,
    )
    return document_request, document_request_key_


def localize_non_usfm_book_names(
    usfm_books: Sequence[USFMBook],
    tn_books: list[TNBook],
    tnc_books: list[TNCBook],
    tq_books: list[TQBook],
    book_names: Mapping[BookCode, str] = BOOK_NAMES,
) -> None:
    """
    In-place replacement of English book names with localized (national) names
    in Translation Notes (TN), Condensed TN (TNC), and Translation Questions (TQ).

    If user has not chosen USFM, gets localized names from USFM for
    given language/book if it was available as a choice in the language chosen.
    """
    replacement_map: ReplacementMap = {}
    # ── Phase 1: Collect known national names from chosen USFM books ──────────
    for usfm in usfm_books:
        english_name = book_names.get(usfm.book_code)
        if not english_name:
            continue
        pattern = re.compile(rf"\b{re.escape(english_name)}\b", re.IGNORECASE)
        replacement_map[(usfm.lang_code, usfm.book_code)] = (
            pattern,
            usfm.national_book_name,
        )

    # book_codes_for_lang_from_usfm_only(lang_code) re-reads and re-parses
    # every USFM file for that language's Bible-text repo, so its result is
    # cached per lang_code here rather than recomputed once per book below.
    usfm_names_by_lang: dict[LangCode, Sequence[tuple[str, str]]] = {}

    def apply_replacement(
        text: str,
        pattern: re.Pattern[str],
        replacement: str,
    ) -> str:
        return pattern.sub(replacement, text)

    def localize_book(
        book: TNBook | TNCBook | TQBook,
        has_book_intro: bool,
        has_chapter_intro: bool,
    ) -> None:
        key: Key = (book.lang_code, book.book_code)
        entry = replacement_map.get(key)
        if entry is None:
            usfm_names = usfm_names_by_lang.get(book.lang_code)
            if usfm_names is None:
                usfm_names = resource_lookup.book_codes_for_lang_from_usfm_only(
                    book.lang_code
                )
                usfm_names_by_lang[book.lang_code] = usfm_names
            match = next(
                (item for item in usfm_names if item[0] == book.book_code), None
            )
            english_name = book_names.get(book.book_code)
            if english_name is None or match is None:
                return
            pattern = re.compile(rf"\b{re.escape(english_name)}\b", re.IGNORECASE)
            replacement = match[1].strip()
            if not replacement:
                return
            replacement_map[key] = (pattern, replacement)
            entry = (pattern, replacement)
        pattern, replacement = entry
        if has_book_intro and hasattr(book, "book_intro"):
            book.book_intro = apply_replacement(book.book_intro, pattern, replacement)
        for chapter in book.chapters.values():
            if has_chapter_intro and hasattr(chapter, "intro_html"):
                chapter.intro_html = apply_replacement(
                    chapter.intro_html, pattern, replacement
                )
            for verse_ref, html in chapter.verses.items():
                chapter.verses[verse_ref] = apply_replacement(
                    html, pattern, replacement
                )

    for tn_book in tn_books:
        localize_book(tn_book, has_book_intro=True, has_chapter_intro=True)
    for tnc_book in tnc_books:
        localize_book(tnc_book, has_book_intro=True, has_chapter_intro=True)
    for tq_book in tq_books:
        localize_book(tq_book, has_book_intro=False, has_chapter_intro=False)


def locate_acquire_and_build_resource_objects(
    document_request: DocumentRequest,
) -> tuple[
    Sequence[ResourceLookupDto],
    Sequence[USFMBook],
    Sequence[TNBook],
    Sequence[TNCBook],
    Sequence[TQBook],
    Sequence[TWBook],
    Sequence[BCBook],
    Sequence[RGBook],
]:
    current_task.update_state(state="Locating assets")
    resource_lookup_dtos = []
    for resource_request in document_request.resource_requests:
        resource_lookup_dto = resource_lookup.resource_lookup_dto(
            resource_request.lang_code,
            resource_request.resource_type,
            resource_request.book_code,
        )
        if resource_lookup_dto:
            resource_lookup_dtos.append(resource_lookup_dto)
    found_resource_lookup_dtos = [
        resource_lookup_dto
        for resource_lookup_dto in resource_lookup_dtos
        if resource_lookup_dto.url is not None
    ]
    current_task.update_state(state="Provisioning asset files")
    t0 = time.time()
    resource_dirs = [
        resource_lookup.prepare_resource_filepath(dto)
        for dto in found_resource_lookup_dtos
    ]
    for resource_dir, dto in zip(resource_dirs, found_resource_lookup_dtos):
        resource_lookup.provision_asset_files(dto.url, resource_dir)
    t1 = time.time()
    logger.info(
        "Time to provision asset files (acquire and write to disk): %s", t1 - t0
    )
    current_task.update_state(state="Parsing asset files")
    t0 = time.time()
    usfm_books, tn_books, tnc_books, tq_books, tw_books, bc_books, rg_books = (
        parsing.books(
            found_resource_lookup_dtos,
            resource_dirs,
            document_request.resource_requests,
            document_request.layout_for_print,
            document_request.use_chapter_labels,
            document_request.generate_docx,
        )
    )
    localize_non_usfm_book_names(
        usfm_books, list(tn_books), list(tnc_books), list(tq_books)
    )
    t1 = time.time()
    logger.info("Time to parse all resource content: %s", t1 - t0)
    return (
        found_resource_lookup_dtos,
        usfm_books,
        tn_books,
        tnc_books,
        tq_books,
        tw_books,
        bc_books,
        rg_books,
    )


@worker.app.task
def generate_document(
    document_request_json: str,
    output_dir: str = settings.DOCUMENT_OUTPUT_DIR,
) -> str:
    """
    This is the main entry point for this module for non-docx generation.
    >>> from doc.domain import document_generator
    >>> document_request_json = '{"email_address":null,"assembly_strategy_kind":"lbo","assembly_layout_kind":"1c","layout_for_print":false,"resource_requests":[{"lang_code":"es-419","resource_type":"ulb","book_code":"mat"}],"generate_pdf":true,"generate_epub":false,"generate_docx":false,"chunk_size":"chapter","limit_words":false,"show_tn_book_intro":false,"document_request_source":"ui"}'
    >>> document_generator.generate_document(document_request_json)
    """
    current_task.update_state(state="Receiving request")
    document_request, document_request_key_ = initialize_document_request_and_key(
        document_request_json
    )
    html_filepath_ = html_filepath(document_request_key_)
    pdf_filepath_ = pdf_filepath(document_request_key_)
    epub_filepath_ = epub_filepath(document_request_key_)
    if file_needs_update(html_filepath_):
        (
            found_resource_lookup_dtos,
            usfm_books,
            tn_books,
            tnc_books,
            tq_books,
            tw_books,
            bc_books,
            rg_books,
        ) = locate_acquire_and_build_resource_objects(document_request)
        current_task.update_state(state="Assembling content")
        document_parts = assemble_content(
            document_request_key_,
            document_request,
            usfm_books,
            tn_books,
            tnc_books,
            tq_books,
            tw_books,
            bc_books,
            rg_books,
        )
        content_str = compose_document(document_parts)
        if usfm_books:
            content_str = check_content_for_issues(content_str)
        content_str = create_title_page_and_wrap_in_template(
            content_str, document_request, found_resource_lookup_dtos, usfm_books
        )
        write_html_content_to_file(content_str, html_filepath_)
    else:
        logger.info("Cache hit for %s", html_filepath_)
    # Immediately return pre-built PDF if the document has previously been
    # generated and is fresh enough.
    if document_request.generate_pdf and file_needs_update(pdf_filepath_):
        current_task.update_state(state="Converting to PDF")
        convert_html_to_pdf(
            html_filepath_,
            pdf_filepath_,
            document_request_key_,
            document_request.use_prince,
        )
        if should_send_email(document_request.email_address):
            attachments = [
                Attachment(filepath=pdf_filepath_, mime_type=("application", "pdf"))
            ]
            current_task.update_state(state="Sending email")
            send_email_with_attachment(
                document_request.email_address,
                attachments,
                document_request_key_,
            )
    if document_request.generate_epub and file_needs_update(epub_filepath_):
        current_task.update_state(state="Converting to ePub")
        convert_html_to_epub(html_filepath_, epub_filepath_, document_request_key_)
        if should_send_email(document_request.email_address):
            attachments = [
                Attachment(
                    filepath=epub_filepath_, mime_type=("application", "epub+zip")
                )
            ]
            current_task.update_state(state="Sending email")
            send_email_with_attachment(
                document_request.email_address,
                attachments,
                document_request_key_,
            )
    return document_request_key_


@worker.app.task
def generate_docx_document(
    document_request_json: str,
    output_dir: str = settings.DOCUMENT_OUTPUT_DIR,
) -> str:
    """
    This is the alternative entry point for Docx document creation only.
    """
    current_task.update_state(state="Receiving request")
    document_request, document_request_key_ = initialize_document_request_and_key(
        document_request_json
    )
    html_filepath_ = html_filepath(document_request_key_)
    docx_filepath_ = docx_filepath(document_request_key_)
    if document_request.generate_docx and file_needs_update(docx_filepath_):
        (
            found_resource_lookup_dtos,
            usfm_books,
            tn_books,
            tnc_books,
            tq_books,
            tw_books,
            bc_books,
            rg_books,
        ) = locate_acquire_and_build_resource_objects(document_request)
        current_task.update_state(state="Assembling content")
        document_parts = assemble_content(
            document_request_key_,
            document_request,
            usfm_books,
            tn_books,
            tnc_books,
            tq_books,
            tw_books,
            bc_books,
            rg_books,
        )
        title1, title2 = get_languages_title_page_strings(
            found_resource_lookup_dtos, usfm_books
        )
        current_task.update_state(state="Converting to Docx")
        convert_html_to_docx(
            html_filepath_,
            docx_filepath_,
            document_parts,
            document_request.layout_for_print,
            title1,
            title2,
        )
        if should_send_email(document_request.email_address):
            attachments = [
                Attachment(
                    filepath=docx_filepath_,
                    mime_type=(
                        "application",
                        "vnd.openxmlformats-officedocument.wordprocessingml.document",
                    ),
                )
            ]
            current_task.update_state(state="Sending email")
            send_email_with_attachment(
                document_request.email_address,
                attachments,
                document_request_key_,
            )
    else:
        logger.info("Cache hit for %s", docx_filepath_)
    return document_request_key_


def document_request_key(
    resource_requests: Sequence[ResourceRequest],
    assembly_strategy_kind: AssemblyStrategyEnum,
    assembly_layout_kind: AssemblyLayoutEnum,
    limit_words: bool,
    use_chapter_labels: bool,
    use_section_visual_separator: bool,
    use_two_column_layout_for_tn_notes: bool,
    use_two_column_layout_for_tq_notes: bool,
    show_tn_book_intro: bool,
    show_bc_book_intro: bool,
    show_tn_chapter_intro: bool,
    show_bc_chapter_commentary: bool,
    show_rg_chapter_commentary: bool,
    max_filename_len: int = 240,
    underscore: str = "_",
    hyphen: str = "-",
) -> str:
    """
    Create and return the document_request_key. The
    document_request_key uniquely identifies a document request.

    If the document request key is max_filename_len or more characters
    in length, then switch to using a shorter string that is based on the
    current time. The reason for this is that the document request key is
    used as the file name (with suffix appended) and each OS has a limit
    to how long a file name may be. max_filename_len should make room for
    the file suffix, e.g., ".html", to be appended.

    It is really useful to have filenames with semantic meaning and so
    those are preferred when possible, i.e., when the file name is not
    too long.
    """
    resource_request_keys = underscore.join(
        [
            hyphen.join(
                [
                    resource_request.lang_code,
                    resource_request.resource_type,
                    resource_request.book_code,
                ]
            )
            for resource_request in resource_requests
        ]
    )
    if any(contains_tw(resource_request) for resource_request in resource_requests):
        document_request_key = f'{resource_request_keys}_{assembly_strategy_kind.value}_{assembly_layout_kind.value}_{"clt" if use_chapter_labels else "clf"}_{"lwt" if limit_words else "lwf"}_{"sst" if use_section_visual_separator else "ssf"}_{"2ctn" if use_two_column_layout_for_tn_notes else "1ctn"}_{"2ctq" if use_two_column_layout_for_tq_notes else "1ctq"}_{"tnbt" if show_tn_book_intro else "tnbf"}_{"bcbt" if show_bc_book_intro else "bcbf"}_{"tnct" if show_tn_chapter_intro else "tncf"}_{"bcct" if show_bc_chapter_commentary else "bccf"}_{"rgct" if show_rg_chapter_commentary else "rgcf"}'
    else:
        document_request_key = f'{resource_request_keys}_{assembly_strategy_kind.value}_{assembly_layout_kind.value}_{"clt" if use_chapter_labels else "clf"}_{"sst" if use_section_visual_separator else "ssf"}_{"2ctn" if use_two_column_layout_for_tn_notes else "1ctn"}_{"2ctq" if use_two_column_layout_for_tq_notes else "1ctq"}_{"tnbt" if show_tn_book_intro else "tnbf"}_{"bcbt" if show_bc_book_intro else "bcbf"}_{"tnct" if show_tn_chapter_intro else "tncf"}_{"bcct" if show_bc_chapter_commentary else "bccf"}_{"rgct" if show_rg_chapter_commentary else "rgcf"}'
    if len(document_request_key) >= max_filename_len:
        # The generated filename could be too long for the OS where this is
        # running. Therefore, use the current time as a document_request_key
        # so that filename is not too long.
        timestamp_components = str(time.time()).split(".")
        return f"{timestamp_components[0]}_{timestamp_components[1]}"
    else:
        # Use the semantic filename which declaratively describes the
        # document request components.
        return document_request_key


def instantiated_html_header_template(
    template_lookup_key: str, title1: str, title2: str, title3: str
) -> str:
    template = env.get_template(template_lookup_key)
    timestring = datetime.now().ctime()
    return template.render(
        timestring=timestring, title1=title1, title2=title2, title3=title3
    )


def enclose_html_content(
    content: str,
    document_html_header: str,
    document_html_footer: str = "</body></html>",
) -> str:
    """
    Write the enclosing HTML header and footer elements around the
    HTML body content for the document.
    """
    return f"{document_html_header}{content}{document_html_footer}"


def document_html_header(
    assembly_layout_kind: Optional[AssemblyLayoutEnum],
    generate_docx: bool,
    title1: str,
    title2: str,
    title3: str,
) -> str:
    if generate_docx:
        template = env.get_template("html/header_no_css_enclosing.html")
        return template.render()
    if assembly_layout_kind and assembly_layout_kind in [
        AssemblyLayoutEnum.ONE_COLUMN_COMPACT,
        AssemblyLayoutEnum.TWO_COLUMN_SCRIPTURE_LEFT_SCRIPTURE_RIGHT_COMPACT,
    ]:
        return instantiated_html_header_template(
            "html/header_compact_enclosing.html", title1, title2, title3
        )
    return instantiated_html_header_template(
        "html/header_enclosing.html", title1, title2, title3
    )


def create_title_page_and_wrap_in_template(
    content: str,
    document_request: DocumentRequest,
    found_resource_lookup_dtos: Sequence[ResourceLookupDto],
    usfm_books: Sequence[USFMBook],
) -> str:
    title1, title2 = get_languages_title_page_strings(
        found_resource_lookup_dtos, usfm_books
    )
    logger.debug("title1: %s, title2: %s", title1, title2)
    title3 = ""
    header = document_html_header(
        document_request.assembly_layout_kind,
        document_request.generate_docx,
        title1,
        title2,
        title3,
    )
    content = enclose_html_content(content, document_html_header=header)
    return content


def assemble_content(
    document_request_key: str,
    document_request: DocumentRequest,
    usfm_books: Sequence[USFMBook],
    tn_books: Sequence[TNBook],
    tnc_books: Sequence[TNCBook],
    tq_books: Sequence[TQBook],
    tw_books: Sequence[TWBook],
    bc_books: Sequence[BCBook],
    rg_books: Sequence[RGBook],
    link_rather_than_include_tw_definitions: bool = settings.LINK_RATHER_THAN_INCLUDE_TW_DEFINITIONS,
) -> list[DocumentPart]:
    """
    Assemble and return the content from all requested resources according to the
    assembly_strategy requested.
    """
    t0 = time.time()
    document_parts: list[DocumentPart] = []
    if (
        document_request.assembly_strategy_kind
        == AssemblyStrategyEnum.INTERLEAVE_BY_BOOK
    ):
        document_parts = assemble_content_by_book(
            usfm_books,
            tn_books,
            tnc_books,
            tq_books,
            tw_books,
            bc_books,
            rg_books,
            cast(AssemblyLayoutEnum, document_request.assembly_layout_kind),
            document_request.use_section_visual_separator,
            document_request.use_two_column_layout_for_tn_notes,
            document_request.use_two_column_layout_for_tq_notes,
            document_request.show_tn_book_intro,
            document_request.show_bc_book_intro,
            document_request.show_tn_chapter_intro,
        )
    elif (
        document_request.assembly_strategy_kind
        == AssemblyStrategyEnum.INTERLEAVE_BY_VERSE_BOOK_AT_A_TIME
    ):
        document_parts = assemble_content_by_verse_book_at_a_time(
            usfm_books,
            tn_books,
            tnc_books,
            tq_books,
            tw_books,
            bc_books,
            rg_books,
            cast(AssemblyLayoutEnum, document_request.assembly_layout_kind),
            document_request.use_section_visual_separator,
            document_request.use_two_column_layout_for_tn_notes,
            document_request.use_two_column_layout_for_tq_notes,
            document_request.show_tn_book_intro,
            document_request.show_bc_book_intro,
            document_request.show_tn_chapter_intro,
            document_request.show_bc_chapter_commentary,
            document_request.show_rg_chapter_commentary,
        )
    elif (
        document_request.assembly_strategy_kind
        == AssemblyStrategyEnum.INTERLEAVE_BY_CHAPTER
    ):
        document_parts = assemble_content_by_chapter(
            usfm_books,
            tn_books,
            tnc_books,
            tq_books,
            tw_books,
            bc_books,
            rg_books,
            cast(AssemblyLayoutEnum, document_request.assembly_layout_kind),
            document_request.use_section_visual_separator,
            document_request.use_two_column_layout_for_tn_notes,
            document_request.use_two_column_layout_for_tq_notes,
            document_request.show_tn_book_intro,
            document_request.show_bc_book_intro,
            document_request.show_tn_chapter_intro,
        )
    elif (
        document_request.assembly_strategy_kind
        == AssemblyStrategyEnum.INTERLEAVE_BY_VERSE
    ):
        document_parts = assemble_content_by_verse_chapter_at_a_time(
            usfm_books,
            tn_books,
            tnc_books,
            tq_books,
            tw_books,
            bc_books,
            rg_books,
            cast(AssemblyLayoutEnum, document_request.assembly_layout_kind),
            document_request.use_section_visual_separator,
            document_request.use_two_column_layout_for_tn_notes,
            document_request.use_two_column_layout_for_tq_notes,
            document_request.show_tn_book_intro,
            document_request.show_bc_book_intro,
            document_request.show_tn_chapter_intro,
            document_request.show_bc_chapter_commentary,
            document_request.show_rg_chapter_commentary,
        )
    t1 = time.time()
    logger.info("Time for interleaving document: %s", t1 - t0)
    if (
        tw_books
        and not link_rather_than_include_tw_definitions
        and not document_request.layout_for_print
    ):
        t0 = time.time()
        # Add the translation words definition section for each language requested.
        unique_tw_books = filter_unique_by_lang_code(tw_books)
        for tw_book in unique_tw_books:
            document_parts.append(
                DocumentPart(
                    content="".join(
                        translation_words_section_for_book(
                            tw_book,
                            usfm_books,
                            False,
                            document_request.resource_requests,
                        )
                    ),
                    is_rtl=tw_book and tw_book.lang_direction == LangDirEnum.RTL,
                    use_section_visual_separator=False,
                )
            )
            document_parts.append(
                two_column_spanning_hr_trick(
                    document_request.use_section_visual_separator
                )
            )
        t1 = time.time()
        logger.info("Time for adding TW content to document: %s", t1 - t0)
    return document_parts


# HTML to PDF converters:
# princexml ($$$$ or non-commercial with watermark) (fastest); we use
# with non-commercial license (watermark on 1st page of pdf). Handles
# layout flawlessly and is dramatically faster than weasyprint and
# all other solutions.
# weasyprint (does a nice job, we also use this),
# wkhtmltopdf via pdfkit (can't handle column-count directive so can't use due to
# multi-column layouts requirement),
# pagedjs-cli (does a really nice job, but is really slow - uses puppeteer underneath),
# electron-pdf (similar speed to wkhtmltopdf) which uses chrome underneath the hood,
# gotenburg which uses chrome under the hood and provides a nice api in Docker (untested),
# raw chrome headless (works well and is about the same speed as weasyprint),
# ebook-convert (faster than weasyprint, but does arbitrary page breaks in formatting and can't do headers and footers)
def convert_html_to_pdf(
    html_filepath: str,
    pdf_filepath: str,
    document_request_key: str,
    use_prince: bool,
    default_converter: str = "weasyprint",
    alternative_converter: str = "prince",
) -> None:
    """
    Generate PDF from HTML and copy it to output directory.
    """
    assert exists(html_filepath)
    logger.info("Generating PDF %s...", pdf_filepath)
    t0 = time.time()
    if use_prince:
        command = [
            alternative_converter,
            html_filepath,
            "-o",
            pdf_filepath,
        ]
    else:
        command = [default_converter, html_filepath, pdf_filepath]
    logger.info("Generate PDF command: %s", " ".join(command))
    subprocess.run(
        command,
        check=True,
        text=True,
    )
    t1 = time.time()
    logger.info("Time for converting HTML to PDF: %s", t1 - t0)


# HTML to ePub converters:
# pandoc (this doesn't respect two column),
# html-to-epub which is written in go (this doesn't respect two column),
# ebook-convert (this respects two column).
def convert_html_to_epub(
    html_filepath: str,
    epub_filepath: str,
    document_request_key: str,
) -> None:
    """Generate ePub from HTML and copy it to output directory."""
    assert exists(html_filepath)
    command = [
        "ebook-convert",
        html_filepath,
        epub_filepath,
        "--no-default-epub-cover",
    ]
    logger.info("Generate ePub command: %s", " ".join(command))
    t0 = time.time()
    subprocess.run(command, check=True, text=True)
    t1 = time.time()
    logger.info("Time for converting HTML to ePub: %s", t1 - t0)


def compose_docx_document(
    document_parts: list[DocumentPart],
) -> DocxDocument:
    """
    Convert a sequence of HTML parts into one DOCX Document,
    performing preprocessing and optional separators.
    """
    doc = Document()
    parser = HtmlToDocx(
        tag_style_overrides={
            "h1": "Title",
        }
    )
    t0 = time.time()
    for part in document_parts:
        if part.contained_in_two_column_section:
            add_two_column_section(doc)
        else:
            add_one_column_section(doc)
        try:
            parser.add_html_to_document(part.content, doc)
        except ValueError as e:
            logger.exception("Error converting HTML to docx: %s", e)
        if part.use_section_visual_separator:
            add_full_width_hr(doc)
        if part.add_page_break:
            add_page_break(doc)
    style_superscripts(doc, lift_half_points=2, color=None)
    # html4doc defeats normal use of hyperlink inline styling in Word
    # via a customized (otherwise standard) Hyperlink style, but
    # this handles it by doing a pass over the document after the fact
    # and forcing hyperlinks to render using a specific style we
    # created to match the PO's desired look.
    # PlainHyperlinkChar is a character style we created in
    # template.docx, but its actual style ID is PlainHyperlinkChar0
    # under the hood.
    override_and_clean_hyperlinks(doc, "PlainHyperlinkChar0")
    t1 = time.time()
    logger.info("Time for converting HTML to Docx: %.2f seconds", t1 - t0)
    return doc


def compose_document(
    document_parts: list[DocumentPart],
    hr: str = "<hr/>",
) -> str:
    content = []
    t0 = time.time()
    for part in document_parts:
        content.append(part.content)
        if part.use_section_visual_separator:
            content.append(hr)
    t1 = time.time()
    logger.info("Time for composing document parts into HTML: %.2f seconds", t1 - t0)
    return "".join(content)


def convert_html_to_docx(
    html_filepath: str,
    docx_filepath: str,
    document_parts: list[DocumentPart],
    layout_for_print: bool,
    title1: str = "title1",
    title2: str = "title2",
    title3: str = "",
    docx_template_path: str = settings.DOCX_TEMPLATE_PATH,
    docx_compact_template_path: str = settings.DOCX_COMPACT_TEMPLATE_PATH,
) -> None:
    """Generate Docx and write it to output directory."""
    t0 = time.time()
    # Get data for front page of Docx template.
    title1 = title1
    title2 = title2
    title3 = title3
    template_path = (
        docx_compact_template_path if layout_for_print else docx_template_path
    )
    doc = DocxTemplate(template_path)
    toc_path = generate_docx_toc(docx_filepath)
    toc = doc.new_subdoc(toc_path)
    context = {
        "title1": title1,
        "title2": title2,
        "title3": title3,
        "TOC": toc,
    }
    doc.render(context)
    # Start new section for different column layout
    new_section = doc.add_section(WD_SECTION.CONTINUOUS)
    new_section.start_type
    master = Composer(doc)
    doc2 = compose_docx_document(document_parts)
    add_internal_docx_links(doc2)
    master.append(doc2)
    master.save(docx_filepath)
    t1 = time.time()
    logger.info("Time for converting HTML to Docx: %s", t1 - t0)


def cover_filepath(
    document_request_key: str, output_dir: str = settings.DOCUMENT_OUTPUT_DIR
) -> str:
    """Given document_request_key, return the HTML cover output file path."""
    return join(output_dir, f"{document_request_key}_cover.html")


def select_assembly_layout_kind(
    document_request: DocumentRequest,
    usfm_resource_types: Sequence[str] = settings.USFM_RESOURCE_TYPES,
    by_book_order: AssemblyStrategyEnum = AssemblyStrategyEnum.INTERLEAVE_BY_BOOK,
    by_verse_book_at_a_time_order: AssemblyStrategyEnum = AssemblyStrategyEnum.INTERLEAVE_BY_VERSE_BOOK_AT_A_TIME,
    by_chapter_order: AssemblyStrategyEnum = AssemblyStrategyEnum.INTERLEAVE_BY_CHAPTER,
    by_verse_order: AssemblyStrategyEnum = AssemblyStrategyEnum.INTERLEAVE_BY_VERSE,
    stet_strategy: AssemblyStrategyEnum = AssemblyStrategyEnum.STET_STRATEGY,
    one_column_compact: AssemblyLayoutEnum = AssemblyLayoutEnum.ONE_COLUMN_COMPACT,
    sl_sr: AssemblyLayoutEnum = AssemblyLayoutEnum.TWO_COLUMN_SCRIPTURE_LEFT_SCRIPTURE_RIGHT,
    sl_sr_compact: AssemblyLayoutEnum = AssemblyLayoutEnum.TWO_COLUMN_SCRIPTURE_LEFT_SCRIPTURE_RIGHT_COMPACT,
    one_column: AssemblyLayoutEnum = AssemblyLayoutEnum.ONE_COLUMN,
    stet_layout: AssemblyLayoutEnum = AssemblyLayoutEnum.STET_LAYOUT,
) -> AssemblyLayoutEnum:
    """
    Make an intelligent choice of what layout to use given the
    DocumentRequest instance the user has requested. Note that prior to
    this, we've already validated the DocumentRequest instance in the
    DocumentRequest's validator. If we hadn't then we wouldn't be able
    to make the assumptions this function makes.
    """
    if (
        document_request.document_request_source == DocumentRequestSourceEnum.TEST
        and document_request.assembly_layout_kind
    ):  # request is coming from a test, so just use the value the test set
        return document_request.assembly_layout_kind
    elif document_request.assembly_strategy_kind == stet_strategy:
        return stet_layout
    elif not document_request.layout_for_print and (
        document_request.assembly_strategy_kind == by_book_order
        or document_request.assembly_strategy_kind == by_verse_book_at_a_time_order
        or document_request.assembly_strategy_kind == by_verse_order
        or document_request.assembly_strategy_kind == by_chapter_order
    ):
        return one_column
    elif document_request.layout_for_print and (
        document_request.assembly_strategy_kind == by_book_order
        or document_request.assembly_strategy_kind == by_verse_book_at_a_time_order
        or document_request.assembly_strategy_kind == by_verse_order
        or document_request.assembly_strategy_kind == by_chapter_order
    ):
        return one_column_compact
    return one_column


def write_html_content_to_file(
    content: str,
    output_filename: str,
) -> None:
    """
    Write HTML content to file.
    """
    logger.info("About to write HTML to %s", output_filename)
    write_file(
        output_filename,
        content,
    )


def check_content_for_issues(
    content: str,
) -> str:
    """
    Check for defects and notify support via logs of possible source
    content issues. Also modify the content to include a message for
    the end user to inform them that there is a problem with the
    underlying source USFM and that the translators need to fix it.
    This will help them understand why they have missing content.
    """
    logger.info(
        "Checking USFM content for issues before creating requested document..."
    )
    if 'class="verse"' not in content:
        logger.info("No verses found in HTML")
        logger.info(
            "About to modify content to include message notifying user of problem with USFM source text format..."
        )
        updated_content = "NOTE: There are issues with the requested underlying scripture USFM text that make it unusable by this system until translators fix the issue for the language(s), book(s), and resource(s) combination you have requested."
        logger.info(
            "Due to potential issues with the source content, here is the HTML content for you to inspect: %s",
            content,
        )
        return updated_content
    return content


def get_languages_title_page_strings(
    resource_lookup_dtos: Sequence[ResourceLookupDto],
    usfm_books: Sequence[USFMBook],
    book_names: dict[str, str] = BOOK_NAMES,
    book_id_map: dict[str, int] = BOOK_ID_MAP,
) -> tuple[str, str]:
    """
    Construct sensical phrases to display for title1 and title2 for
    first page of Word document.
    """
    lang_codes = list(dict.fromkeys(dto.lang_code for dto in resource_lookup_dtos))

    def get_language_details(lang_code: str) -> str:
        book_names_ = []
        resource_type_names = []
        dtos = [dto for dto in resource_lookup_dtos if dto.lang_code == lang_code]
        for dto in dtos:
            usfm_books_ = [
                usfm_book
                for usfm_book in usfm_books
                if usfm_book.book_code == dto.book_code
                and usfm_book.lang_code == lang_code
            ]
            if usfm_books_:
                book_name = usfm_books_[0].national_book_name
            else:
                book_name = book_names[dto.book_code]
            if book_name not in book_names_:
                book_names_.append(book_name)
            if dto.resource_type_name not in resource_type_names:
                resource_type_names.append(dto.resource_type_name)
        if dtos:
            return f"{dtos[0].lang_name} ({dtos[0].localized_lang_name}): {', '.join(resource_type_names)} for {', '.join(book_names_)}"
        return ""

    lang0_title = get_language_details(lang_codes[0]) if lang_codes else ""
    lang1_title = get_language_details(lang_codes[1]) if len(lang_codes) > 1 else ""
    return lang0_title, lang1_title


if __name__ == "__main__":
    # To run the doctests in the this module, in the root of the project do:
    # FROM_EMAIL_ADDRESS=... python backend/doc/domain/resource_lookup.py
    # See https://docs.python.org/3/library/doctest.html
    # for more details.
    import doctest

    doctest.testmod()
