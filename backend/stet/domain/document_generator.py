from collections import Counter, defaultdict
from datetime import datetime
from typing import Mapping, Sequence, cast

import mistune
from celery import current_task
from doc.config import settings
from doc.domain import worker
from doc.domain.email_utils import send_email_with_attachment, should_send_email
from doc.domain.model import Attachment
from doc.domain.parsing import (
    lookup_verse_text,
    usfm_book_content,
)
from doc.domain.resource_lookup import (
    prepare_resource_filepath,
    provision_asset_files,
    resource_lookup_dto,
    resource_types,
)
from doc.utils.file_utils import docx_filepath, file_needs_update
from doc.utils.text_utils import maybe_correct_book_name
from docx import Document
from docx.enum.text import WD_PARAGRAPH_ALIGNMENT
from docx.oxml import OxmlElement
from docx.oxml.ns import qn
from html4docx import HtmlToDocx  # type: ignore
from pydantic import Json
from stet.domain.model import PassageEntry, WordEntry
from stet.domain.parser import get_word_entry_dtos
from stet.utils.html_utils import clean_chapter_html
from doc.domain.parsing import split_into_verses
from stet.domain.strings import (
    LOCALIZED_DATE_FORMAT_STRINGS,
    TRANSLATED_FOOTER_PHRASES_TABLE,
    TRANSLATED_HEADER_PHRASES_TABLE,
    TRANSLATED_TABLE_COLUMN_HEADERS,
)
from stet.utils.docx_utils import (
    add_footer,
    add_header,
    add_highlighted_html_to_docx_for_words,
    add_lined_page_at_end,
    add_plain_html_to_docx,
    add_preformatted_html_to_docx,
    adjust_table_columns,
    reduce_spacing_around_tables,
)
from stet.utils.util import extract_chapter_and_beyond

logger = settings.logger(__name__)


def generate_docx_document(
    lang0_code: str,
    lang1_code: str,
    use_increased_line_spacing: bool,
    document_request_key_: str,
    docx_filepath_: str,
    usfm_resource_types: Sequence[str] = settings.USFM_RESOURCE_TYPES,
    resource_type_codes_and_names: Mapping[
        str, str
    ] = settings.RESOURCE_TYPE_CODES_AND_NAMES,
    preferred_resources: Mapping[str, str] = settings.PREFERRED_RESOURCES,
) -> str:
    """
    Generate the scriptural terms evaluation document.

    >>> from stet.domain.document_generator import generate_docx_document
    >>> generate_docx_document()
    """
    word_entries: list[WordEntry] = []
    word_entry_dtos, lang0_book_codes_and_names = get_word_entry_dtos(
        lang0_code, lang1_code
    )
    lang0_resource_types = resource_types(
        lang0_code,
        ",".join(
            [book_code_and_name[0] for book_code_and_name in lang0_book_codes_and_names]
        ),
    )
    lang0_resource_types_ = [
        lang0_resource_type_tuple[0]
        for lang0_resource_type_tuple in lang0_resource_types
    ]
    lang1_resource_types = resource_types(
        lang1_code,
        ",".join(
            [book_code_and_name[0] for book_code_and_name in lang0_book_codes_and_names]
        ),
    )
    lang1_resource_types_ = [
        lang1_resource_type_tuple[0]
        for lang1_resource_type_tuple in lang1_resource_types
    ]
    lang0_usfm_resource_types = [
        resource_type_
        for resource_type_ in lang0_resource_types_
        if resource_type_ in usfm_resource_types
    ]
    lang1_usfm_resource_types = [
        resource_type_
        for resource_type_ in lang1_resource_types_
        if resource_type_ in usfm_resource_types
    ]
    lang0_ulb_usfm_resource_types = [
        usfm_resource_type_
        for usfm_resource_type_ in lang0_usfm_resource_types
        if "ulb" in usfm_resource_type_
    ]
    lang1_ulb_usfm_resource_types = [
        usfm_resource_type_
        for usfm_resource_type_ in lang1_usfm_resource_types
        if "ulb" in usfm_resource_type_
    ]
    source_usfm_books = []
    target_usfm_books = []

    def resolve_resource_type(
        lang_code: str,
        available_types: list[str],
        ulb_types: list[str],
        preferred_map: Mapping[str, str],
    ) -> str:
        preferred_type = preferred_map.get(lang_code)
        if preferred_type and preferred_type in available_types:
            return preferred_type
        if ulb_types:
            return ulb_types[0]
        if available_types:
            return available_types[0]
        return ""

    lang0_usfm_resource_type = resolve_resource_type(
        lang0_code,
        lang0_usfm_resource_types,
        lang0_ulb_usfm_resource_types,
        preferred_resources,
    )
    lang1_usfm_resource_type = resolve_resource_type(
        lang1_code,
        lang1_usfm_resource_types,
        lang1_ulb_usfm_resource_types,
        preferred_resources,
    )
    if lang0_usfm_resource_type and lang1_usfm_resource_type:
        source_usfm_book = None
        target_usfm_book = None
        for book_code, book_name in lang0_book_codes_and_names:
            current_task.update_state(state="Locating assets")
            lang0_resource_lookup_dto_ = resource_lookup_dto(
                lang0_code, lang0_usfm_resource_type, book_code
            )
            if lang0_resource_lookup_dto_ and lang0_resource_lookup_dto_.url:
                current_task.update_state(state="Provisioning asset files")
                lang0_resource_dir = prepare_resource_filepath(
                    lang0_resource_lookup_dto_
                )
                provision_asset_files(
                    lang0_resource_lookup_dto_.url, lang0_resource_dir
                )
                current_task.update_state(state="Parsing asset files")
                source_usfm_book = usfm_book_content(
                    lang0_resource_lookup_dto_,
                    lang0_resource_dir,
                    False,
                )
                for (
                    chapter_num_,
                    chapter_,
                ) in source_usfm_book.chapters.items():
                    # STET needs additional specialized cleaning
                    cleaned_source_chapter = clean_chapter_html(chapter_.content)
                    # Since we cleaned specially, let's re-generate
                    # the verses from the cleaned HTML.
                    chapter_.verses = split_into_verses(cleaned_source_chapter)
                source_usfm_books.append(source_usfm_book)
            lang1_resource_lookup_dto_ = resource_lookup_dto(
                lang1_code, lang1_usfm_resource_type, book_code
            )
            if lang1_resource_lookup_dto_ and lang1_resource_lookup_dto_.url:
                lang1_resource_dir = prepare_resource_filepath(
                    lang1_resource_lookup_dto_
                )
                provision_asset_files(
                    lang1_resource_lookup_dto_.url, lang1_resource_dir
                )
                target_usfm_book = usfm_book_content(
                    lang1_resource_lookup_dto_,
                    lang1_resource_dir,
                    False,
                )
                for (
                    chapter_num_,
                    chapter_,
                ) in target_usfm_book.chapters.items():
                    # STET needs additional specialized cleaning
                    cleaned_target_chapter = clean_chapter_html(chapter_.content)
                    # Since we cleaned specially, let's re-generate
                    # the verses from the cleaned HTML.
                    chapter_.verses = split_into_verses(cleaned_target_chapter)
                target_usfm_books.append(target_usfm_book)
    # Count total occurrences per reference (using source_reference as key)
    reference_counter: Counter[str] = Counter()
    for word_entry_dto in word_entry_dtos:
        for passage_ref_dto in word_entry_dto.passage_ref_dtos:
            # If one verse_ref_dto can contain multiple verses → count them
            ref = passage_ref_dto.source_reference
            reference_counter[ref] += len(passage_ref_dto.verse_refs)
    # Track current occurrence number as we process
    occurrence_tracker: defaultdict[str, int] = defaultdict(int)
    current_task.update_state(state="Assembling content")
    for word_entry_dto in word_entry_dtos:
        source_passage_text = ""
        target_passage_text = ""
        word_entry = WordEntry()
        word_entry.words = word_entry_dto.words
        word_entry.bolded_phrases = word_entry_dto.bolded_phrases
        word_entry.strongs_numbers = word_entry_dto.strongs_numbers
        word_entry.definition = cast(str, mistune.markdown(word_entry_dto.definition))
        for passage_ref_dto in word_entry_dto.passage_ref_dtos:
            source_selected_usfm_books = [
                usfm_book_
                for usfm_book_ in source_usfm_books
                if usfm_book_.lang_code == lang0_code
                and usfm_book_.book_code == passage_ref_dto.book_code
                and usfm_book_.resource_type_name
                == resource_type_codes_and_names[lang0_usfm_resource_type]
            ]
            target_selected_usfm_books = [
                usfm_book_
                for usfm_book_ in target_usfm_books
                if usfm_book_.lang_code == lang1_code
                and usfm_book_.book_code == passage_ref_dto.book_code
                and usfm_book_.resource_type_name
                == resource_type_codes_and_names[lang1_usfm_resource_type]
            ]
            source_passage_text = ""
            target_passage_text = ""
            source_selected_usfm_book = None
            target_selected_usfm_book = None
            if source_selected_usfm_books:
                source_selected_usfm_book = source_selected_usfm_books[0]
                source_selected_usfm_book.national_book_name = maybe_correct_book_name(
                    lang0_code, source_selected_usfm_book.national_book_name
                )
            if target_selected_usfm_books:
                target_selected_usfm_book = target_selected_usfm_books[0]
                target_selected_usfm_book.national_book_name = maybe_correct_book_name(
                    lang1_code, target_selected_usfm_book.national_book_name
                )
                logger.debug(
                    "target_usfm_book.national_book_name: %s",
                    target_selected_usfm_book.national_book_name,
                )
            non_book_name_portion_of_source_reference = extract_chapter_and_beyond(
                passage_ref_dto.source_reference
            )
            non_book_name_portion_of_target_reference = extract_chapter_and_beyond(
                passage_ref_dto.target_reference
            )
            localized_source_reference = (
                f"{source_selected_usfm_book.national_book_name} {non_book_name_portion_of_source_reference}"
                if source_selected_usfm_book
                and non_book_name_portion_of_source_reference
                else passage_ref_dto.source_reference
            )
            localized_target_reference = (
                f"{target_selected_usfm_book.national_book_name} {non_book_name_portion_of_target_reference}"
                if target_selected_usfm_book
                and non_book_name_portion_of_target_reference
                else passage_ref_dto.target_reference
            )
            for verse_ref in passage_ref_dto.verse_refs:
                if passage_ref_dto.source_text_with_bolding is not None:
                    source_passage_text = passage_ref_dto.source_text_with_bolding
                elif source_selected_usfm_book:
                    source_passage_text = lookup_verse_text(
                        source_selected_usfm_book,
                        passage_ref_dto.chapter_num,
                        verse_ref.strip(),
                    )
                else:
                    source_passage_text = ""
                if target_selected_usfm_book:
                    target_passage_text = lookup_verse_text(
                        target_selected_usfm_book,
                        passage_ref_dto.chapter_num,
                        verse_ref.strip(),
                    )
                else:
                    target_passage_text = ""
            # Occurrence logic
            ref_key = passage_ref_dto.source_reference
            total = reference_counter[ref_key]
            occurrence_tracker[ref_key] += 1
            current = occurrence_tracker[ref_key]
            word_entry.passage_entries.append(
                PassageEntry(
                    source_reference=localized_source_reference,
                    source_text=source_passage_text,
                    target_reference=localized_target_reference,
                    target_text=target_passage_text,
                    occurrence_index=current,
                    occurrence_total=total,
                    source_text_with_bolding=passage_ref_dto.source_text_with_bolding,
                )
            )
        word_entries.append(word_entry)
    current_task.update_state(state="Converting to Docx")
    generate_docx(
        word_entries, docx_filepath_, lang0_code, lang1_code, use_increased_line_spacing
    )
    return docx_filepath_


def generate_docx(
    word_entries: list[WordEntry],
    docx_filepath: str,
    lang0_code: str,
    lang1_code: str,
    use_increased_line_spacing: bool,
    translated_table_column_headers: dict[
        str, tuple[str, str, str, str]
    ] = TRANSLATED_TABLE_COLUMN_HEADERS,
    translated_footer_phrases_table: dict[str, str] = TRANSLATED_FOOTER_PHRASES_TABLE,
    localized_date_format_strings: dict[str, str] = LOCALIZED_DATE_FORMAT_STRINGS,
    translated_header_phrases_table: dict[str, str] = TRANSLATED_HEADER_PHRASES_TABLE,
) -> None:
    """
    Generates a DOCX document from a list of word entries and saves it to the given file path.
    :param word_entries: A list of word entries containing the word, strongs numbers, definition, and verses.
    :param docx_filepath: The file path where the generated DOCX document will be saved.
    :param lang0_code: Source language code for the document header.
    :param lang1_code: Target language code for the document header.
    :param use_increased_line_spacing: Indicate whether to increase line spacing to double that of normal.
    """
    doc = Document()
    html_to_docx = HtmlToDocx()
    for word_entry in word_entries:
        # Add the word heading
        heading: str = (
            f"{','.join(word_entry.words)} ({word_entry.strongs_numbers})"
            if word_entry.strongs_numbers
            else "".join(word_entry.words)
        )
        doc.add_heading(heading, level=1)
        # Convert the HTML definition to DOCX content
        if word_entry.definition:
            html_to_docx.add_html_to_document(word_entry.definition, doc)
        # Create a table with three columns
        table = doc.add_table(rows=1, cols=3)
        table.style = "Table Grid"
        # Set the header of the table and apply bold formatting
        hdr_cells = table.rows[0].cells
        hdr_cells[0].text = translated_table_column_headers[lang0_code][0]
        hdr_cells[1].text = translated_table_column_headers[lang0_code][1]
        hdr_cells[2].text = translated_table_column_headers[lang0_code][2]
        hdr_cells[2].paragraphs[0].alignment = WD_PARAGRAPH_ALIGNMENT.CENTER
        for hdr_cell in hdr_cells:
            hdr_cell.paragraphs[0].runs[0].bold = True
        # Add verses to the table
        for passage_entry in word_entry.passage_entries:
            # Row for references
            row_cells = table.add_row().cells
            source_ref_display = passage_entry.source_reference
            if passage_entry.occurrence_total > 1:
                source_ref_display += f" ({passage_entry.occurrence_index}/{passage_entry.occurrence_total})"
            target_ref_display = passage_entry.target_reference
            if passage_entry.occurrence_total > 1:
                target_ref_display += f" ({passage_entry.occurrence_index}/{passage_entry.occurrence_total})"
            source_paragraph = row_cells[0].paragraphs[0]
            source_run = source_paragraph.add_run(passage_entry.source_reference)
            source_run.bold = True
            if passage_entry.occurrence_total > 1:
                occurrence_run = source_paragraph.add_run(
                    f" ({passage_entry.occurrence_index}/{passage_entry.occurrence_total})"
                )
                occurrence_run.bold = True
                occurrence_run.italic = True
            target_paragraph = row_cells[1].paragraphs[0]
            target_run = target_paragraph.add_run(passage_entry.target_reference)
            target_run.bold = True
            if passage_entry.occurrence_total > 1:
                occurrence_run = target_paragraph.add_run(
                    f" ({passage_entry.occurrence_index}/{passage_entry.occurrence_total})"
                )
                occurrence_run.bold = True
                occurrence_run.italic = True
            status_run = (
                row_cells[2]
                .paragraphs[0]
                .add_run(translated_table_column_headers[lang0_code][3])
            )
            status_run.bold = True
            row_cells[2].paragraphs[0].alignment = WD_PARAGRAPH_ALIGNMENT.CENTER
            # Row for texts
            row_cells = table.add_row().cells
            # Process HTML content in source_text and highlight keyword
            source_paragraph = row_cells[0].paragraphs[0]
            if use_increased_line_spacing:
                source_paragraph.paragraph_format.line_spacing = (
                    2.0  # Adjust line spacing
                )
            if passage_entry.source_text_with_bolding:
                add_preformatted_html_to_docx(
                    passage_entry.source_text_with_bolding, source_paragraph
                )
            elif len(word_entry.bolded_phrases) > 0:
                add_highlighted_html_to_docx_for_words(
                    passage_entry.source_text,
                    source_paragraph,
                    word_entry.bolded_phrases,
                )
            else:  # Bolded phrases in 4th column were not provided
                add_highlighted_html_to_docx_for_words(
                    passage_entry.source_text, source_paragraph, word_entry.words
                )
            # Add target_text with wider line spacing
            target_paragraph = row_cells[1].paragraphs[0]
            if use_increased_line_spacing:
                target_paragraph.paragraph_format.line_spacing = (
                    2.0  # Adjust line spacing
                )
            add_plain_html_to_docx(passage_entry.target_text, target_paragraph)
            # Vertically centered Unicode checkbox
            checkbox_cell = row_cells[2]
            checkbox_paragraph = checkbox_cell.paragraphs[0]
            checkbox_paragraph.text = "\u2610"
            checkbox_paragraph.alignment = WD_PARAGRAPH_ALIGNMENT.CENTER
            tc = checkbox_cell._tc  # Access the XML element of the table cell
            tcPr = tc.get_or_add_tcPr()  # Get or add the cell properties
            vAlign = OxmlElement("w:vAlign")  # Create the vertical alignment element
            vAlign.set(qn("w:val"), "center")  # Set alignment to "center"
            tcPr.append(vAlign)  # Append the vertical alignment to cell properties
        # Adjust column widths to prioritize the first two columns
        adjust_table_columns(table)
    footer_phrase = translated_footer_phrases_table[lang0_code]
    current_datetime = datetime.now().strftime(
        localized_date_format_strings[lang0_code]
    )
    date_text = f"{footer_phrase} {current_datetime}"
    doc = add_footer(doc, date_text)
    header_phrase = translated_header_phrases_table[lang0_code]
    doc = add_header(doc, lang0_code, lang1_code, header_phrase)
    doc = add_lined_page_at_end(doc)
    reduce_spacing_around_tables(doc)
    doc.save(docx_filepath)


@worker.app.task
def generate_stet_docx_document(
    lang0_code: str,
    lang1_code: str,
    email_address: str,
    use_increased_line_spacing: bool,
) -> Json[str]:
    logger.debug(
        "passed args: lang0_code: %s, lang1_code: %s, email_adress: %s, use_increased_line_spacing: %s",
        lang0_code,
        lang1_code,
        email_address,
        use_increased_line_spacing,
    )
    document_request_key_ = (
        f"{lang0_code}_{lang1_code}_{'2l' if use_increased_line_spacing else '1l'}_stet"
    )
    docx_filepath_ = docx_filepath(document_request_key_)
    if file_needs_update(docx_filepath_):
        generate_docx_document(
            lang0_code,
            lang1_code,
            use_increased_line_spacing,
            document_request_key_,
            docx_filepath_,
        )
        if should_send_email(email_address):
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
                email_address,
                attachments,
                document_request_key_,
            )
    else:
        logger.debug("Cache hit for %s", docx_filepath_)
    return document_request_key_
