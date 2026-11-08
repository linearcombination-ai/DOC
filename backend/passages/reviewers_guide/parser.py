import os
import re
from collections import defaultdict
from pprint import pprint

from doc.config import get_settings
from doc.domain.bible_books import BOOK_NAMES
from doc.domain.model import ChapterNum, LangDirEnum
from passages.reviewers_guide.model import (
    RGBook,
    RGChapter,
    Part1Item,
    Part2Item,
    BibleReference,
    ParsedText,
)
from docx import Document

# Pattern for chapter and verse references after Bible books
CHAPTER_VERSE_PATTERN = re.compile(r"^\d+:\d+(-\d+(:\d+)?)?$")

logger = get_settings().logger(__name__)


def get_book_code(book_name: str, book_names: dict[str, str] = BOOK_NAMES) -> str:
    return next(key for key, name in book_names.items() if name == book_name)


def parse_bible_reference(
    raw_bible_reference: str,
    book_names: list[str] = list(BOOK_NAMES.values()),
    exceptional_book_name: str = "Song of Songs",
) -> BibleReference:
    bible_reference_components = raw_bible_reference.split()
    if (
        bible_reference_components
        and len(bible_reference_components) == 3
        and (
            f"{bible_reference_components[0]} {bible_reference_components[1]}"
            in book_names
        )
        and ":" in bible_reference_components[2]
        and bible_reference_components[2][0].isdigit()
    ):
        book_name = f"{bible_reference_components[0]} {bible_reference_components[1]}"
        book_code = get_book_code(book_name)
        chapter_verse_components = bible_reference_components[2].split(":")
        logger.debug("chapter_verse_components: %s", chapter_verse_components)
        if len(chapter_verse_components) == 3 and "-" in chapter_verse_components[1]:
            # e.g., 1 Corinthians 3:4-4:12 -> chapter_verse_components = ["3", "4-4", "12"]
            bible_reference = get_bible_reference_spanning_chapter_boundary(
                chapter_verse_components, book_code, book_name
            )
        else:
            # e.g., 1 Corinthians 3:12-14 -> chapter_verse_components = ["3", "12-14"]
            # or
            # e.g., 1 Corinthians 3:12 -> chapter_verse_components = ["3", "12"]
            bible_reference = get_ordinal_bible_reference(
                chapter_verse_components, book_code, book_name
            )
    elif (
        bible_reference_components
        and len(bible_reference_components) == 2
        and bible_reference_components[0] in book_names
        and ":" in bible_reference_components[1]
        and bible_reference_components[1][0].isdigit()
    ):
        book_name = bible_reference_components[0]
        book_code = get_book_code(book_name)
        chapter_verse_components = bible_reference_components[1].split(":")
        if len(chapter_verse_components) == 3 and "-" in chapter_verse_components[1]:
            # e.g., Samuel 4:3-6:10 -> chapter_verse_components = ["4", "3-6", "10"]
            bible_reference = get_bible_reference_spanning_chapter_boundary(
                chapter_verse_components, book_code, book_name
            )
        else:
            # e.g., Samuel 4:3-6 -> chapter_verse_components = ["4", "3-6"]
            bible_reference = get_ordinal_bible_reference(
                chapter_verse_components, book_code, book_name
            )
    else:
        logger.info("Likely parsing an OT rg bible reference")
        if len(bible_reference_components) >= 3:
            if bible_reference_components[2][0].isdigit():
                # Likely an OT reviewer's guide reference which sometimes looks like ['1', 'Samuel', '1:1-2:3', 'foo', 'bar']
                bible_reference_components = bible_reference_components[0:3]
                logger.debug(
                    "1. Updated bible_reference_components: %s",
                    bible_reference_components,
                )
                book_name = (
                    f"{bible_reference_components[0]} {bible_reference_components[1]}"
                )
                book_code = get_book_code(book_name)
                chapter_verse_components = bible_reference_components[2].split(":")
                bible_reference = get_bible_reference_spanning_chapter_boundary(
                    chapter_verse_components, book_code, book_name
                )
            elif bible_reference_components[1][
                0
            ].isdigit() and not bible_reference_components[2].endswith("continued"):
                bible_reference_components = bible_reference_components[0:2]
                logger.debug(
                    "2. Updated bible_reference_components: %s",
                    bible_reference_components,
                )
                book_name = bible_reference_components[0]
                book_code = get_book_code(book_name)
                chapter_verse_components = bible_reference_components[1].split(":")
                if (
                    len(chapter_verse_components) == 3
                    and "-" in chapter_verse_components[1]
                ):
                    bible_reference = get_bible_reference_spanning_chapter_boundary(
                        chapter_verse_components, book_code, book_name
                    )
                else:
                    bible_reference = get_ordinal_bible_reference(
                        chapter_verse_components, book_code, book_name
                    )
            elif (
                not bible_reference_components[2].endswith("continued")
                and f"{bible_reference_components[0]} {bible_reference_components[1]} {bible_reference_components[2]} == {exceptional_book_name}"
            ):
                bible_reference_components = bible_reference_components[0:4]
                book_code = get_book_code(
                    f"{bible_reference_components[0]} {bible_reference_components[1]} Solomon"
                )
                chapter_verse_components = bible_reference_components[3].split(":")
                if (
                    len(chapter_verse_components) == 3
                    and "-" in chapter_verse_components[1]
                ):
                    bible_reference = get_bible_reference_spanning_chapter_boundary(
                        chapter_verse_components, book_code, exceptional_book_name
                    )
                else:
                    bible_reference = get_ordinal_bible_reference(
                        chapter_verse_components, book_code, exceptional_book_name
                    )
    return bible_reference


def get_ordinal_bible_reference(
    chapter_verse_components: list[str], book_code: str, book_name: str
) -> BibleReference:
    start_chapter = chapter_verse_components[0]
    start_chapter_verse_ref = chapter_verse_components[1]
    bible_reference = BibleReference(
        lang_code=None,
        book_code=book_code,
        book_name=book_name,
        start_chapter=int(start_chapter),
        start_chapter_verse_ref=start_chapter_verse_ref,
        end_chapter=None,
        end_chapter_verse_ref=None,
    )
    return bible_reference


def get_bible_reference_spanning_chapter_boundary(
    chapter_verse_components: list[str], book_code: str, book_name: str
) -> BibleReference:
    # e.g., Samuel 4:3-6:10 -> chapter_verse_components = ["4", "3-6", "10"]
    start_chapter = chapter_verse_components[0]
    verse_chapter_components = chapter_verse_components[1].split("-")
    # e.g., verse_chapter_components = ["3", "6"]
    start_chapter_verse_ref = (
        verse_chapter_components[0]
        if len(chapter_verse_components) >= 3
        else chapter_verse_components[1]
    )
    end_chapter = (
        verse_chapter_components[1] if len(chapter_verse_components) >= 3 else None
    )
    end_chapter_verse_ref = (
        chapter_verse_components[2] if len(chapter_verse_components) >= 3 else None
    )
    bible_reference = BibleReference(
        lang_code=None,
        book_code=book_code,
        book_name=book_name,
        start_chapter=int(start_chapter),
        start_chapter_verse_ref=start_chapter_verse_ref,
        end_chapter=int(end_chapter) if end_chapter else None,
        end_chapter_verse_ref=end_chapter_verse_ref,
    )
    return bible_reference


def find_bible_references(
    docx_file: str,
    book_names: list[str] = list(BOOK_NAMES.values()),
    solomon: str = "Solomon",
) -> tuple[list[str], list[str]]:
    """
    Identifies Bible passage references and text between references.

    Bible passage references:
        - Start with a Bible book name.
        - Are followed by a valid chapter and verse reference, e.g., "3:16-17".
        - Do not end with the word "continued".

    Args:
        docx_file (str): Path to the Word (.docx) file.

    Returns:
        tuple[list[str], list[str]]:
            - The first list contains text found between Bible references.
            - The second list contains the Bible references themselves.
    """
    doc = Document(docx_file)
    between_texts: list[str] = []
    bible_references: list[str] = []
    current_text: list[str] = []
    inside_bible_reference: bool = False
    testament_paragraph = doc.paragraphs[2]
    if "Old Testament" in testament_paragraph.text.strip():
        logger.info("Parsing Old Testament rg document")
        for paragraph in doc.paragraphs:
            paragraph_text = paragraph.text.strip()
            words = paragraph_text.split()
            if (
                words
                and len(words) > 1
                and (words[0] in book_names or (f"{words[0]} {words[1]}" in book_names))
                and (
                    words[1][0].isdigit() or (len(words) > 2 and words[2][0].isdigit())
                )
            ) or (
                words
                and len(words) > 3
                and (
                    f"{words[0]} {words[1]} {solomon}" in book_names
                )  # Input doc uses Song of Songs, but lookup is Song of Solomon
                and words[3][0].isdigit()
            ):
                if paragraph_text.endswith("continued"):
                    continue
                if inside_bible_reference:
                    if current_text:
                        between_texts.append(" ".join(current_text))
                        current_text = []
                bible_references.append(paragraph_text)
                inside_bible_reference = True
                continue
            if inside_bible_reference:
                current_text.append(paragraph_text)
    else:
        logger.info("Parsing New Testament rg document")
        for paragraph in doc.paragraphs:
            paragraph_text = paragraph.text.strip()
            words = paragraph_text.split()
            if (
                words
                and len(words) > 1
                and len(words) < 5
                and (words[0] in book_names or (f"{words[0]} {words[1]}" in book_names))
            ):
                if paragraph_text.endswith("continued") or "\t" in paragraph_text:
                    continue
                if inside_bible_reference:
                    if current_text:
                        between_texts.append(" ".join(current_text))
                        current_text = []
                bible_references.append(paragraph_text)
                inside_bible_reference = True
                continue
            if inside_bible_reference:
                current_text.append(paragraph_text)
    if current_text:
        between_texts.append(" ".join(current_text))
    return between_texts, bible_references


def get_last_sentence(text: str) -> str:
    # Use regex to match sentences instead of splitting, so punctuation is preserved
    sentences = re.findall(r"[^\s].*?[.?!](?=\s|$)", text.strip())
    # Return the last sentence if any are found, or an empty string otherwise
    return sentences[-1] if sentences else ""


def get_first_sentence(text: str) -> str:
    # Use regex to match sentences instead of splitting, so punctuation is preserved
    sentences = re.findall(r"[^\s].*?[.?!](?=\s|$)", text.strip())
    # Return the last sentence if any are found, or an empty string otherwise
    return sentences[0] if sentences else ""


def parse_text(text: str, raw_bible_reference: str) -> ParsedText:
    """
    Parses the in-between text for a Bible passage reference into a structured data type.

    Args:
        text (str): The raw in-between text to be parsed.

    Returns:
        ParsedText: A class containing the parsed content.
    """
    bible_reference = parse_bible_reference(raw_bible_reference)
    text = text.replace(f"{raw_bible_reference} continued", "")
    sections = re.split(r"(Background:|Part 1|Part 2|Comment Section:)", text)
    current_section = None
    background = None
    directive = ""
    comment_section = None
    for section in sections:
        section = section.strip()
        if section == "Background:":
            current_section = "background"
        elif section == "Part 1":
            current_section = "part_1"
        elif section == "Part 2":
            current_section = "part_2"
        elif section == "Comment Section:":
            current_section = "comment_section"
        else:
            if current_section == "background":
                directive = get_last_sentence(section)
                background = (
                    section.removesuffix(directive).rstrip() if directive else section
                )
            elif current_section == "part_1":
                part_1 = []
                part1_directive = get_first_sentence(section)
                bullets = re.findall(
                    r"(.*?)\s+\[(\d+:\d+(?:-\d+)?)\]",
                    section.removeprefix(part1_directive),
                )
                for bullet in bullets:
                    part_1.append(
                        Part1Item(text=bullet[0].strip(), reference=f"[{bullet[1]}]")
                    )
            elif current_section == "part_2":
                part_2 = []
                part2_directive = get_first_sentence(section)
                qa_pattern = re.compile(
                    r"\[((?:\d+:\d+(?:-\d+)?(?:, \d+)*))\]\s+([^\?\n]*)\?([^\[\n]*)"
                    r"(?=\s*\[\d+:\d+(?:-\d+)?(?:, \d+)*\])",
                    re.DOTALL,
                )
                questions_answers = qa_pattern.findall(
                    section.removeprefix(part2_directive)
                )
                for qa in questions_answers:
                    part_2.append(
                        Part2Item(
                            reference=f"[{qa[0]}]",
                            question=f"{qa[1].strip()}?",
                            answer=qa[2].strip(),
                        )
                    )
            elif current_section == "comment_section":
                comment_section = None
            else:
                if section and background is None:
                    directive = section
    return ParsedText(
        bible_reference=bible_reference,
        background=background,
        directive=directive,
        part_1=part_1,
        part_1_directive=part1_directive,
        part_2=part_2,
        part_2_directive=part2_directive,
        comment_section=comment_section,
    )


def books(parsed_objects: list[ParsedText]) -> set[str]:
    return {po.bible_reference.book_code for po in parsed_objects if po.bible_reference}


def create_rgbooks_from_parsed_texts(
    parsed_texts: list[ParsedText],
    lang_code: str,
    lang_name: str,
    resource_type_name: str,
    lang_direction: LangDirEnum,
) -> list[RGBook]:
    # Group ParsedText by book_code and then by chapter
    books: dict[str, dict[ChapterNum, list[ParsedText]]] = defaultdict(
        lambda: defaultdict(list)
    )
    for parsed_text in parsed_texts:
        if parsed_text.bible_reference:
            book_code = parsed_text.bible_reference.book_code
            chapter = parsed_text.bible_reference.start_chapter
            books[book_code][chapter].append(parsed_text)
    rgbooks: list[RGBook] = []
    for book_code, chapters_dict in books.items():
        chapters = {
            chapter_num: RGChapter(content=chapter_texts)
            for chapter_num, chapter_texts in chapters_dict.items()
        }
        rgbook = RGBook(
            lang_code=lang_code,
            lang_name=lang_name,
            book_code=book_code,
            resource_type_name=resource_type_name,
            chapters=chapters,
            lang_direction=lang_direction,
        )
        rgbooks.append(rgbook)
    return rgbooks


# Parsed, language-independent content of each .docx, keyed by file path.
# An entry is only valid for the (mtime, size) it was parsed at, so a file
# that is replaced on disk is reparsed without restarting the process.
_parsed_texts_cache: dict[str, tuple[tuple[int, int], list[ParsedText]]] = {}


def parsed_texts_from_docx(docx_file_path: str) -> list[ParsedText]:
    """
    Return the parsed passages of a Reviewer's Guide .docx. Reading and
    parsing the .docx is the costly part, so the language-independent
    result is cached in memory until the file's modification time or size
    changes. Callers get their own copies, since they localize and
    otherwise mutate what they receive.
    """
    stat = os.stat(docx_file_path)
    signature = (stat.st_mtime_ns, stat.st_size)
    cached = _parsed_texts_cache.get(docx_file_path)
    if cached is None or cached[0] != signature:
        between_texts, bible_references = find_bible_references(docx_file_path)
        parsed = [
            parse_text(text, ref) for text, ref in zip(between_texts, bible_references)
        ]
        cached = (signature, parsed)
        _parsed_texts_cache[docx_file_path] = cached
    return [parsed_text.model_copy(deep=True) for parsed_text in cached[1]]


def get_rg_books(
    docx_file_path: str,
    lang_code: str,
    lang_name: str,
    resource_type_name: str,
    lang_direction: LangDirEnum,
) -> list[RGBook]:
    parsed_objects = parsed_texts_from_docx(docx_file_path)
    rg_books = create_rgbooks_from_parsed_texts(
        parsed_objects, lang_code, lang_name, resource_type_name, lang_direction
    )
    return rg_books


if __name__ == "__main__":
    docx_file_path = "backend/passages/data/en_rg/en_rg_nt_survey.docx"
    rg_books = get_rg_books(
        docx_file_path, "en", "English", "Reviewers' Guide NT Survey", LangDirEnum.LTR
    )
    for rg_book in rg_books:
        pprint(rg_book)
    docx_file_path = "backend/passages/data/en_rg/en_ot_survey_rg1_gen_deu.docx"
    rg_books = get_rg_books(
        docx_file_path,
        "en",
        "English",
        "Reviewers' Guide OT Survey RG1",
        LangDirEnum.LTR,
    )
    for rg_book in rg_books:
        pprint(rg_book)
    docx_file_path = "backend/passages/data/en_rg/en_ot_survey_rg2_jos_est.docx"
    rg_books = get_rg_books(
        docx_file_path,
        "en",
        "English",
        "Reviewers' Guide OT Survey RG2",
        LangDirEnum.LTR,
    )
    for rg_book in rg_books:
        pprint(rg_book)
    docx_file_path = "backend/passages/data/en_rg/en_ot_survey_rg3_job_sng.docx"
    rg_books = get_rg_books(
        docx_file_path,
        "en",
        "English",
        "Reviewers' Guide OT Survey RG3",
        LangDirEnum.LTR,
    )
    for rg_book in rg_books:
        pprint(rg_book)
    docx_file_path = "backend/passages/data/en_rg/en_ot_survey_rg4_isa_mal.docx"
    rg_books = get_rg_books(
        docx_file_path,
        "en",
        "English",
        "Reviewers' Guide OT Survey RG4",
        LangDirEnum.LTR,
    )
    for rg_book in rg_books:
        pprint(rg_book)

    text1 = "Hello world! How are you doing today? This is the final sentence."
    assert get_last_sentence(text1) == "This is the final sentence."

    text2 = "Another example sentence without issues."
    assert get_last_sentence(text2) == "Another example sentence without issues."

    text3 = "No punctuation here"
    assert get_last_sentence(text3) == ""

    text4 = "Single word."
    assert get_last_sentence(text4) == "Single word."

    text5 = "Empty input:"
    assert get_last_sentence("") == ""

    text6 = "End with whitespace?   "
    assert get_last_sentence(text6) == "End with whitespace?"

    part1_directive = get_first_sentence(
        "This is a directive. I think you can see what I mean."
    )
    assert part1_directive == "This is a directive."
