"""
This module provides an API for parsing content.
"""

from bs4 import BeautifulSoup, NavigableString
from re import (
    compile,
    escape,
    findall,
    search,
    split as re_split,
    sub,
    DOTALL,
    MULTILINE,
    Pattern,
)
import time
from glob import glob
from os import DirEntry, scandir, walk
from os.path import exists, join, split
from pathlib import Path
from typing import Mapping, Optional, Sequence, cast

import mistune
import requests
from bs4 import BeautifulSoup
from doc.config import settings
from doc.utils.text_utils import demote_headings_by_one
from doc.domain.bible_books import BOOK_ID_MAP, BOOK_NAMES
from doc.domain.model import (
    BC_RESOURCE_TYPE,
    EN_TN_CONDENSED_RESOURCE_TYPE,
    RG_RESOURCE_TYPE,
    TN_RESOURCE_TYPE,
    TQ_RESOURCE_TYPE,
    TW_RESOURCE_TYPE,
    BCBook,
    BCChapter,
    ChapterNum,
    ResourceLookupDto,
    ResourceRequest,
    TNBook,
    TNCBook,
    TNChapter,
    TNCChapter,
    TQBook,
    TQChapter,
    TWBook,
    TWNameContentPair,
    USFMBook,
    USFMChapter,
    VerseRef,
)
from doc.domain.usfm_error_detection_and_fixes import (
    RESOURCES_WITH_USFM_DEFECTS,
    fix_usfm,
)
from doc.markdown_transforms.markdown_transformer import (
    remove_sections,
    transform_ta_and_tn_links,
    transform_tw_links,
    remove_pagination_symbols,
)
from doc.reviewers_guide.model import RGBook
from doc.reviewers_guide.parser import get_rg_books
from doc.utils.text_utils import (
    maybe_correct_book_name,
    chapter_label_numeric_part,
    chapter_label_sans_numeric_part,
    normalize_localized_book_name,
)
from doc.utils.tw_utils import (
    localized_translation_word,
    translation_word_filepaths,
    translation_words_dict,
    tw_resource_dir,
)
from doc.utils.url_utils import (
    get_last_segment,
    get_book_name_from_title_file,
    book_codes_and_names_from_manifest,
)
from pydantic import HttpUrl

logger = settings.logger(__name__)

H1, H2, H3, H4, H5 = "h1", "h2", "h3", "h4", "h5"

SECTIONHEAD5_RE = compile(r'<div\s+class="sectionhead-5">\s*</div>')
EMPTY_P_RE = compile(r"<p>\s*</p>")

BC_ARTICLE_URL_FMT_STR: str = (
    "https://content.bibletranslationtools.org/WycliffeAssociates/en_bc/src/branch/master/{}"
)


CHAPTER_LABEL_REGEX = compile(r"\\cl\s+[^\n]+")
CHAPTER_LABEL_REGEX2 = compile(r"\\cl\s+(.+)")
CHAPTER_REGEX = compile(r"\\c\s+\d+")
CHAPTER_CAPTURE_REGEX = compile(r"(\\c\s+\d+)")
CHAPTER_CAPTURE_REGEX2 = compile(r"\\c\s+(\d+)")


def find_usfm_files(
    resource_dir: str,
    usfm_glob_fmt_str: str = "{}**/*.usfm",
    usfm_ending_in_txt_glob_fmt_str: str = "{}**/*.txt",
    usfm_ending_in_txt_in_subdirectory_glob_fmt_str: str = "{}**/**/*.txt",
) -> list[str]:
    usfm_files = glob(usfm_glob_fmt_str.format(resource_dir))
    if not usfm_files:
        # USFM files sometimes have txt suffix instead of usfm
        usfm_files = glob(usfm_ending_in_txt_glob_fmt_str.format(resource_dir))
        # Sometimes the txt USFM files live at another location
        if not usfm_files:
            usfm_files = glob(
                usfm_ending_in_txt_in_subdirectory_glob_fmt_str.format(resource_dir)
            )
    # Exclude "title.txt" from the results
    usfm_files = [
        filepath for filepath in usfm_files if not filepath.endswith("title.txt")
    ]
    return usfm_files


def filter_usfm_files(
    content_files: list[str],
    book_code: str,
    usfm_suffix: str = ".usfm",
    txt_suffix: str = ".txt",
) -> list[str]:
    suffix_of_content_files = str(Path(content_files[0]).suffix)
    if suffix_of_content_files == usfm_suffix:
        return [
            content_file
            for content_file in content_files
            if book_code.lower() in str(Path(content_file).stem).lower()
        ]
    elif suffix_of_content_files == txt_suffix:
        return [
            content_file
            for content_file in content_files
            if book_code.lower() in str(content_file).lower()
        ]
    return []


def write_usfm_content_to_file(content: str, filepath_sans_suffix: str) -> str:
    filepath = f"{filepath_sans_suffix}.usfm"
    with open(filepath, "w") as fp:
        fp.write(content)
    return filepath


def print_directory_contents(directory: str) -> None:
    """
    Useful for debugging layout on Github Action virtual machine
    """
    for root, dirs, files in walk(directory):
        logger.debug("Directory: %s", root)
        for file in files:
            logger.debug("  File: %s", file)
        for dir in dirs:
            logger.debug("  Subdirectory: %s", dir)


def convert_usfm_chapter_to_html(
    content: str,
    input_file: str,
    output_file: str,
    api_url: str = "http://usfmparserapi:80/api/converter/convert",
) -> None:
    """
    Invoke the dotnet USFM parser through an HTTP POST request to parse the USFM file,
    and render it into HTML and store it on disk.
    """
    logger.info("About to convert USFM to HTML via HTTP POST")
    payload = {
        "InputFile": input_file,
        "OutputFile": output_file,
    }
    try:
        response = requests.post(api_url, json=payload)
        # response.raise_for_status()  # Raise an error for 4xx/5xx responses
        if response.json():
            logger.info("Conversion successful: %s", output_file)
        else:
            logger.error("Conversion failed with an unknown error.")
    except requests.exceptions.RequestException as e:
        logger.error("HTTP request failed: %s", e)


def usfm_asset_file(
    resource_lookup_dto: ResourceLookupDto,
    resource_dir: str,
    use_chapter_labels: bool,
    usfm_glob_fmt_str: str = "{}**/*.usfm",
    usfm_ending_in_txt_glob_fmt_str: str = "{}**/*.txt",
    usfm_ending_in_txt_in_subdirectory_glob_fmt_str: str = "{}**/**/*.txt",
) -> Optional[str]:
    """
    Find the USFM asset and return its path as string or
    None if path not found.
    """
    usfm_files = find_usfm_files(resource_dir)
    filtered_usfm_files: list[str] = []
    if usfm_files:
        filtered_usfm_files = filter_usfm_files(
            usfm_files, resource_lookup_dto.book_code
        )
    if filtered_usfm_files:
        logger.debug("filtered_usfm_files: %s", filtered_usfm_files)
        # A USFM git repo can have each USFM chapter in a separate directory and
        # each verse span in a separate file in that directory. We concatenate the
        # book's USFM files into one USFM file.
        if len(filtered_usfm_files) > 1:
            return combine_usfm_files(
                resource_dir,
                resource_lookup_dto,
                use_chapter_labels,
            )
        else:
            return filtered_usfm_files[0]
    return None


def usfm_chapter_html(
    content: str,
    input_file: str,
    output_file: str,
    chapter_num: int,
) -> Optional[str]:
    t0 = time.time()
    with open(input_file, "w") as f:
        f.write(content)
    convert_usfm_chapter_to_html(content, input_file, output_file)
    t1 = time.time()
    logger.info(
        "Time to convert USFM to HTML for %s: %s",
        output_file,
        t1 - t0,
    )
    html_content = ""
    if exists(output_file):
        with open(output_file, "r") as fs:
            html_content = fs.read()
        return html_content
    return None


def remove_links(html: str) -> str:
    """
    Turn HTML links into spans
    """
    html = html.replace("<a ", "<span ").replace("</a>", "</span>")
    return html


def split_usfm_by_chapters(
    lang_code: str,
    resource_type: str,
    book_code: str,
    usfm_text: str,
    check_usfm: bool = settings.CHECK_USFM,
    chapter_regex: Pattern[str] = CHAPTER_REGEX,
    resources_with_usfm_defects: Sequence[
        tuple[str, str, str]
    ] = RESOURCES_WITH_USFM_DEFECTS,
    check_all_books_for_language: bool = settings.CHECK_ALL_BOOKS_FOR_LANGUAGE,
) -> tuple[str, list[str], list[str]]:
    r"""
    Split the USFM text into chapters
    """
    chapter_markers = []
    chapters = []
    chapter_markers = findall(chapter_regex, usfm_text)
    chapters = re_split(chapter_regex, usfm_text)
    frontmatter = chapters.pop(0).strip()

    defective_lang_codes = {resource[0] for resource in resources_with_usfm_defects}

    def needs_fixing() -> bool:
        """
        Determine whether this resource should be checked for known USFM defects.

        If CHECK_ALL_BOOKS_FOR_LANGUAGE is enabled, then the presence of any
        known defective resource for a language causes all books for that
        language to be checked for similar defects. Otherwise, only explicitly
        listed resource tuples are checked.
        """
        if check_all_books_for_language:
            return lang_code in defective_lang_codes
        return (
            lang_code,
            resource_type,
            book_code,
        ) in resources_with_usfm_defects

    updated_chapters = []
    for marker, chapter in zip(chapter_markers, chapters):
        stripped_chapter = chapter.lstrip()
        # logger.debug("stripped_chapter[0:60]: %s", stripped_chapter[0:60])
        if stripped_chapter:
            if check_usfm and needs_fixing():
                stripped_chapter = fix_usfm(
                    stripped_chapter, lang_code, resource_type, book_code
                )
                # updated_chapter = marker + "\n" + stripped_chapter
                # logger.debug("updated_chapter[0:60]: %s", updated_chapter[0:60])
            updated_chapters.append(marker + "\n" + stripped_chapter)
    return frontmatter, chapter_markers, updated_chapters


def ensure_chapter_label(
    chapter_usfm_text: str,
    chapter_num: int,
    chapter_label_regex: Pattern[str] = CHAPTER_LABEL_REGEX,
    chapter_regex: Pattern[str] = CHAPTER_REGEX,
) -> str:
    r"""
    Modify USFM source to insert an English chapter label if it does not have one.
    Ensure that the chapter label includes the chapter number.
    """
    if not search(chapter_label_regex, chapter_usfm_text):
        if search(chapter_regex, chapter_usfm_text):
            chapter_usfm_text = sub(
                r"(\\c\s+\d+)",
                "\n" + r"\1" + "\n" + r"\\cl Chapter " + f"{chapter_num}" + "\n",
                chapter_usfm_text,
            )
            return chapter_usfm_text
    # Ensure chapter label contains the chapter number
    match = search(r"\\cl\s+(.+)", chapter_usfm_text)
    if match:
        label_text = match.group(1)
        if str(chapter_num) not in label_text:
            updated_label = f"{escape(label_text)} {chapter_num}"
            chapter_usfm_text = sub(
                r"\\cl\s+(.+)",
                rf"\\cl {updated_label}",
                chapter_usfm_text,
            )
            return chapter_usfm_text
    logger.debug(
        "Chapter label already existed and contained the chapter number, didn't modify it"
    )
    return chapter_usfm_text


def ensure_no_chapter_labels(
    chapter_usfm_text: str,
    chapter_label_regex: Pattern[str] = CHAPTER_LABEL_REGEX,
) -> str:
    r"""
    Modify USFM source to remove all chapter labels, \cl.
    """
    if search(chapter_label_regex, chapter_usfm_text):
        updated_chapter_usfm_text = sub(
            chapter_label_regex,
            "",
            chapter_usfm_text,
        )
        return updated_chapter_usfm_text
    return chapter_usfm_text


def get_chapter_num(
    chapter_usfm_text: str,
    chapter_regex: Pattern[str] = CHAPTER_CAPTURE_REGEX2,
) -> int:
    """Get the chapter number from the USFM chapter source text."""
    if match := search(chapter_regex, chapter_usfm_text):
        chapter_num = match.group(1)
        return int(chapter_num)
    return -1  # return sentinal


def remove_null_bytes_and_control_characters(html_content: Optional[str]) -> str:
    """
    Remove any NULL bytes and all control characters.

    Some languages' accidentally have ASCI control characters in their
    USFM. We strip those out as well as the possibility of ASCII NULL
    bytes.
    """
    return sub(r"[\x00-\x1F]+", "", html_content) if html_content else ""


def extract_usfm_frontmatter(frontmatter: str) -> dict[str, str]:
    patterns = {
        "h": r"\\h\s+(.*?)(?=\s+\\|\n|$)",
        "mt": r"\\mt\s+(.*?)(?=\s+\\|\n|$)",
        "toc1": r"\\toc1\s+(.*?)(?=\s+\\|\n|$)",
        "toc2": r"\\toc2\s+(.*?)(?=\s+\\|\n|$)",
    }
    extracted_data = {}
    for key, pattern in patterns.items():
        match = search(pattern, frontmatter, MULTILINE)
        if match:
            extracted_data[key] = match.group(1).strip()
    return extracted_data


# Global defaults for fallback/reference
DEFAULT_BOOK_NAME_LOOKUP_ORDER = ["h", "mt", "toc1", "toc2"]

SPECIALIZED_BOOK_NAME_LOOKUP_MAP: dict[tuple[str, str], list[str]] = {
    ("fr", "f10"): ["toc2"],
}

# Define combinations that should skip normalization
SKIP_NORMALIZATION_SET: set[tuple[str, str]] = {
    ("fr", "f10"),  # Skip normalization for French f10
}


def maybe_localized_book_name(
    frontmatter: str,
    language: str,
    resource_type: str,
    default_book_name_lookup_order: list[str] = DEFAULT_BOOK_NAME_LOOKUP_ORDER,
    specialized_book_name_lookup_map: dict[
        tuple[str, str], list[str]
    ] = SPECIALIZED_BOOK_NAME_LOOKUP_MAP,
    skip_normalization_set: set[tuple[str, str]] = SKIP_NORMALIZATION_SET,
) -> str:
    """
    Rule for obtaining localized book name based on language and resource type.
    Falls back to empirical default sequence if no specialization exists.
    Allows skipping normalization for specific language/resource combinations.
    """
    frontmatter_data = extract_usfm_frontmatter(frontmatter)
    # Normalize inputs for lookup consistency
    lang_key = language.lower()
    res_key = resource_type.lower()
    lookup_key = (lang_key, res_key)
    # 1. Determine the marker lookup order (Specialized vs Default)
    marker_order = specialized_book_name_lookup_map.get(
        lookup_key, default_book_name_lookup_order
    )
    # 2. Iterate through the preferred markers and grab the first one that exists
    localized_book_name = ""
    for marker in marker_order:
        value = frontmatter_data.get(marker)
        if value:
            localized_book_name = value
            break
    logger.debug(
        "Using marker order %s for (%s, %s). Found: %s",
        marker_order,
        language,
        resource_type,
        localized_book_name,
    )
    # 3. Normalize and clean up if a name was found and not explicitly skipped
    if localized_book_name:
        if lookup_key in skip_normalization_set:
            logger.debug("Skipping normalization for %s", lookup_key)
        else:
            localized_book_name = normalize_localized_book_name(localized_book_name)
    return localized_book_name


def ensure_chapter_marker(
    chapter_usfm_text: str,
    chapter_num: int,
    chapter_regex: Pattern[str] = CHAPTER_CAPTURE_REGEX,
) -> str:
    r"""
    Modify USFM source to insert a chapter marker, \c <chapter_num>, if it does not have one.
    """
    if search(chapter_regex, chapter_usfm_text):
        logger.debug("Chapter marker already existed, didn't add one")
        return chapter_usfm_text
    logger.debug("Chapter marker is missing, adding one...")
    # Try inserting before \cl, if present
    if match := search(r"\\cl\s+[^\n]+", chapter_usfm_text):
        insert_pos = match.start()
        return (
            chapter_usfm_text[:insert_pos]
            + f"\n\\c {chapter_num}\n"
            + chapter_usfm_text[insert_pos:]
        )
    # Otherwise, insert at the beginning
    return f"\\c {chapter_num}\n" + chapter_usfm_text


def remove_unwanted_elements(
    content: str,
    sectionhead5_re: Pattern[str] = SECTIONHEAD5_RE,
    empty_paragraph_re: Pattern[str] = EMPTY_P_RE,
) -> str:
    result = sectionhead5_re.sub(" ", content)
    return empty_paragraph_re.sub("", result)


def usfm_book_content(
    resource_lookup_dto: ResourceLookupDto,
    resource_dir: str,
    use_chapter_labels: bool,
    book_names: Mapping[str, str] = BOOK_NAMES,
    working_dir: str = settings.WORKING_DIR,
    use_localized_book_name: bool = settings.USE_LOCALIZED_BOOK_NAME,
) -> USFMBook:
    """
    First produce HTML content from USFM content and then break the
    HTML content returned into a model.USFMBook data structure for use
    during interleaving with other resource assets.
    """
    content_file = usfm_asset_file(
        resource_lookup_dto,
        resource_dir,
        use_chapter_labels,
    )
    content = ""
    if content_file and exists(content_file):
        with open(content_file, "r") as f:
            content = f.read()
    if not use_chapter_labels:
        content = ensure_no_chapter_labels(content)
    usfm_chapters: dict[ChapterNum, USFMChapter] = {}
    frontmatter, chapter_markers, chapters_usfm = split_usfm_by_chapters(
        resource_lookup_dto.lang_code,
        resource_lookup_dto.resource_type,
        resource_lookup_dto.book_code,
        content,
    )
    localized_book_name = ""
    if use_localized_book_name:
        localized_book_name = get_localized_book_name(
            frontmatter, resource_dir, resource_lookup_dto
        )
        localized_book_name = maybe_correct_book_name(
            resource_lookup_dto.lang_code, localized_book_name
        )
        logger.debug("localized_book_name: %s", localized_book_name)
    for chapter_marker, chapter_usfm in zip(chapter_markers, chapters_usfm):
        chapter_num = get_chapter_num(chapter_usfm)
        if chapter_num == -1:
            chapter_num = chapter_label_numeric_part(chapter_usfm)
        if use_chapter_labels:
            chapter_usfm = ensure_chapter_label(chapter_usfm, chapter_num)
        chapter_usfm = ensure_chapter_marker(chapter_usfm, chapter_num)
        resource_filename_sans_suffix = "_".join(
            [
                resource_lookup_dto.lang_code,
                resource_lookup_dto.resource_type,
                resource_lookup_dto.book_code,
                str(chapter_num),
            ]
        )
        resource_filepath_sans_suffix = join(working_dir, resource_filename_sans_suffix)
        input_file = f"{resource_filepath_sans_suffix}.usfm"
        output_file = f"{resource_filepath_sans_suffix}.html"
        chapter_html_content = usfm_chapter_html(
            chapter_usfm, input_file, output_file, chapter_num
        )
        cleaned_chapter_html_content_ = remove_null_bytes_and_control_characters(
            chapter_html_content
        )
        cleaned_chapter_html_content = clean_content_html(cleaned_chapter_html_content_)
        usfm_chapters[chapter_num] = USFMChapter(
            content=(
                cleaned_chapter_html_content if cleaned_chapter_html_content else ""
            ),
            verses=None,
        )
    return USFMBook(
        lang_code=resource_lookup_dto.lang_code,
        lang_name=resource_lookup_dto.lang_name,
        localized_lang_name=resource_lookup_dto.localized_lang_name,
        book_code=resource_lookup_dto.book_code,
        national_book_name=(
            localized_book_name
            if localized_book_name
            else book_names[resource_lookup_dto.book_code]
        ),
        resource_type_name=resource_lookup_dto.resource_type_name,
        chapters=usfm_chapters if usfm_chapters else {},
        lang_direction=resource_lookup_dto.lang_direction,
    )


def get_localized_book_name(
    frontmatter: str,
    resource_dir: str,
    resource_lookup_dto: ResourceLookupDto,
    usfm_resource_types: Sequence[str] = settings.USFM_RESOURCE_TYPES,
) -> str:
    localized_book_name = maybe_localized_book_name(
        frontmatter,
        resource_lookup_dto.lang_code,
        resource_lookup_dto.resource_type,
    )
    if not localized_book_name:
        book_codes_and_names_from_manifest_ = book_codes_and_names_from_manifest(
            resource_dir
        )
        localized_book_name = book_codes_and_names_from_manifest_.get(
            resource_lookup_dto.book_code, ""
        )
        if not localized_book_name:
            last_segment = get_last_segment(
                # We know that url is not null because of how we got here
                cast(HttpUrl, resource_lookup_dto.url),
                resource_lookup_dto.lang_code,
            )
            repo_components = last_segment.split("_")
            if (
                len(repo_components) > 2
                and resource_lookup_dto.resource_type in usfm_resource_types
            ):
                localized_book_name = get_book_name_from_title_file(
                    resource_dir, resource_lookup_dto.lang_code, repo_components
                )
    return localized_book_name


def load_manifest(file_path: str) -> str:
    with open(file_path, "r") as file:
        return file.read()


def glob_chapter_dirs(
    resource_dir: str,
    book_code: str,
    glob_in_subdirs_fmt_str: str = "{}/**/*{}/*[0-9]*",
    glob_fmt_str: str = "{}/*{}/*[0-9]*",
) -> list[str]:
    chapter_dirs = glob(glob_in_subdirs_fmt_str.format(resource_dir, book_code))
    # Some languages are organized differently on disk
    if not chapter_dirs:
        chapter_dirs = glob(
            glob_in_subdirs_fmt_str.format(resource_dir, book_code.upper())
        )
    if not chapter_dirs:
        chapter_dirs = glob(glob_fmt_str.format(resource_dir, book_code))
    if not chapter_dirs:
        chapter_dirs = glob(glob_fmt_str.format(resource_dir, book_code.upper()))
    return sorted(chapter_dirs)


def tn_chapter_verses(
    resource_dir: str,
    lang_code: str,
    book_code: str,
    resource_requests: Sequence[ResourceRequest],
) -> dict[int, TNChapter]:
    chapter_dirs = sorted(glob_chapter_dirs(resource_dir, book_code))
    chapter_verses = {}
    for chapter_dir in chapter_dirs:
        chapter_num = int(Path(chapter_dir).name)
        chapter_intro = tn_chapter_intro(chapter_dir)
        chapter_intro_html = ""
        if chapter_intro:
            chapter_intro = remove_sections(chapter_intro)
            tw_resource_dir_ = tw_resource_dir(lang_code)
            translation_words_dict_ = translation_words_dict(tw_resource_dir_)
            chapter_intro = transform_tw_links(
                chapter_intro,
                lang_code,
                resource_requests,
                translation_words_dict_,
            )
            chapter_intro_with_updated_links = transform_ta_and_tn_links(
                chapter_intro,
                lang_code,
                resource_requests,
            )
            chapter_intro_html_raw = cast(
                str, mistune.markdown(chapter_intro_with_updated_links)
            )
            chapter_intro_html = remove_pagination_symbols(chapter_intro_html_raw)
        verses_html = tn_verses_html(
            chapter_dir, lang_code, book_code, resource_requests
        )
        chapter_verses[chapter_num] = TNChapter(
            intro_html=chapter_intro_html, verses=verses_html
        )
    return chapter_verses


def tnc_chapter_verses(
    resource_dir: str,
    lang_code: str,
    book_code: str,
    resource_requests: Sequence[ResourceRequest],
) -> dict[int, TNCChapter]:
    chapter_dirs = sorted(glob_chapter_dirs(resource_dir, book_code))
    chapter_verses = {}
    for chapter_dir in chapter_dirs:
        chapter_num = int(Path(chapter_dir).name)
        chapter_intro = tn_chapter_intro(chapter_dir)
        chapter_intro_html = ""
        if chapter_intro:
            chapter_intro = remove_sections(chapter_intro)
            tw_resource_dir_ = tw_resource_dir(lang_code)
            translation_words_dict_ = translation_words_dict(tw_resource_dir_)
            chapter_intro = transform_tw_links(
                chapter_intro,
                lang_code,
                resource_requests,
                translation_words_dict_,
            )
            chapter_intro_with_updated_links = transform_ta_and_tn_links(
                chapter_intro,
                lang_code,
                resource_requests,
            )
            chapter_intro_html_raw = cast(
                str, mistune.markdown(chapter_intro_with_updated_links)
            )
            chapter_intro_html = remove_pagination_symbols(chapter_intro_html_raw)
        verses_html = tn_verses_html(
            chapter_dir, lang_code, book_code, resource_requests
        )
        chapter_verses[chapter_num] = TNCChapter(
            intro_html=chapter_intro_html, verses=verses_html
        )
    return chapter_verses


def tn_chapter_intro(
    chapter_dir: str,
    glob_md_fmt_str: str = "{}/*intro.md",
    glob_txt_fmt_str: str = "{}/*intro.txt",
) -> Optional[str]:
    intro_paths = sorted(glob(glob_md_fmt_str.format(chapter_dir)))
    if not intro_paths:
        intro_paths = sorted(glob(glob_txt_fmt_str.format(chapter_dir)))
    if intro_paths:
        with open(intro_paths[0], "r") as f:
            return f.read()
    else:
        return None


def book_intro_markdown(resource_dir: str, book_code: str) -> str:
    book_intro_paths = sorted(glob(f"{resource_dir}/*{book_code}/front/intro.md"))
    if not book_intro_paths:
        book_intro_paths = sorted(glob(f"{resource_dir}/*{book_code}/front/intro.txt"))
    if book_intro_paths and exists(book_intro_paths[0]):
        with open(book_intro_paths[0], "r") as f:
            return f.read()
    else:
        return ""


def tn_verses_html(
    chapter_dir: str,
    lang_code: str,
    book_code: str,
    resource_requests: Sequence[ResourceRequest],
    book_names: Mapping[str, str] = BOOK_NAMES,
    verse_fmt_str: str = "<h4>{} {}:{}</h4>\n{}",
    glob_md_fmt_str: str = "{}/*[0-9]*.md",
    glob_txt_fmt_str: str = "{}/*[0-9]*.txt",
    h1: str = H1,
    h5: str = H5,
) -> dict[VerseRef, str]:
    verse_paths = sorted(glob(glob_md_fmt_str.format(chapter_dir)))
    if not verse_paths:
        verse_paths = sorted(glob(glob_txt_fmt_str.format(chapter_dir)))
    verses_html = {}
    for filepath in verse_paths:
        verse_ref = Path(filepath).stem
        with open(filepath, "r") as f:
            verse_md_content = f.read()
            verse_md_content = transform_ta_and_tn_links(
                verse_md_content,
                lang_code,
                resource_requests,
            )
            verse_html_content = cast(str, mistune.markdown(verse_md_content))
            adjusted_verse_html_content = sub(h1, h5, verse_html_content)
            verses_html[verse_ref] = verse_fmt_str.format(
                # NOTE Use nationalized book name from usfm book if available rather
                # than English book name as here - we accompish this later in
                # document_generator > localize_non_usfm_book_names
                book_names[book_code],
                int(Path(chapter_dir).stem),
                int(verse_ref),
                adjusted_verse_html_content,
            )
    return verses_html


def tn_book_content(
    resource_lookup_dto: ResourceLookupDto,
    resource_dir: str,
    resource_requests: Sequence[ResourceRequest],
    layout_for_print: bool,
    show_tn_book_intro: bool = settings.SHOW_TN_BOOK_INTRO,
) -> TNBook:
    chapter_verses = tn_chapter_verses(
        resource_dir,
        resource_lookup_dto.lang_code,
        resource_lookup_dto.book_code,
        resource_requests,
    )
    book_intro = ""
    if show_tn_book_intro:
        book_intro = book_intro_markdown(resource_dir, resource_lookup_dto.book_code)
        if book_intro:
            book_intro = remove_sections(book_intro)
            tw_resource_dir_ = tw_resource_dir(resource_lookup_dto.lang_code)
            translation_words_dict_ = translation_words_dict(tw_resource_dir_)
            book_intro = transform_tw_links(
                book_intro,
                resource_lookup_dto.lang_code,
                resource_requests,
                translation_words_dict_,
            )
            book_intro = transform_ta_and_tn_links(
                book_intro,
                resource_lookup_dto.lang_code,
                resource_requests,
            )
            book_intro = cast(str, mistune.markdown(book_intro))
    return TNBook(
        lang_code=resource_lookup_dto.lang_code,
        lang_name=resource_lookup_dto.lang_name,
        book_code=resource_lookup_dto.book_code,
        resource_type_name=resource_lookup_dto.resource_type_name,
        book_intro=book_intro,
        chapters=chapter_verses,
        lang_direction=resource_lookup_dto.lang_direction,
    )


def tnc_book_content(
    resource_lookup_dto: ResourceLookupDto,
    resource_dir: str,
    resource_requests: Sequence[ResourceRequest],
    layout_for_print: bool,
    show_tn_book_intro: bool = settings.SHOW_TN_BOOK_INTRO,
) -> TNCBook:
    chapter_verses = tnc_chapter_verses(
        resource_dir,
        resource_lookup_dto.lang_code,
        resource_lookup_dto.book_code,
        resource_requests,
    )
    book_intro = ""
    if show_tn_book_intro:
        book_intro = book_intro_markdown(resource_dir, resource_lookup_dto.book_code)
        if book_intro:
            book_intro = remove_sections(book_intro)
            tw_resource_dir_ = tw_resource_dir(resource_lookup_dto.lang_code)
            translation_words_dict_ = translation_words_dict(tw_resource_dir_)
            book_intro = transform_tw_links(
                book_intro,
                resource_lookup_dto.lang_code,
                resource_requests,
                translation_words_dict_,
            )
            book_intro = transform_ta_and_tn_links(
                book_intro,
                resource_lookup_dto.lang_code,
                resource_requests,
            )
            book_intro = cast(str, mistune.markdown(book_intro))
    return TNCBook(
        lang_code=resource_lookup_dto.lang_code,
        lang_name=resource_lookup_dto.lang_name,
        book_code=resource_lookup_dto.book_code,
        resource_type_name=resource_lookup_dto.resource_type_name,
        book_intro=book_intro,
        chapters=chapter_verses,
        lang_direction=resource_lookup_dto.lang_direction,
    )


def clean_numeric_string(s: str) -> str:
    """
    Removes all non-numeric characters from the input string.

    Args:
        s (str): Input string that may contain non-numeric characters.

    Returns:
        str: String containing only numeric characters.
    """
    return "".join(c for c in s if c.isdigit())


def tq_chapter_verses(
    resource_dir: str,
    lang_code: str,
    book_code: str,
    resource_requests: Sequence[ResourceRequest],
    book_names: Mapping[str, str] = BOOK_NAMES,
    verse_paths_glob_fmt_str: str = "{}/*[0-9]*.md",
    h1: str = H1,
    h5: str = H5,
    verse_label_fmt_str: str = "<h4>{} {}:{}</h4>\n{}",
) -> dict[int, TQChapter]:
    chapter_dirs = sorted(glob_chapter_dirs(resource_dir, book_code))
    chapter_verses = {}
    for chapter_dir in chapter_dirs:
        chapter_num = int(split(chapter_dir)[-1])
        verse_paths = sorted(glob(verse_paths_glob_fmt_str.format(chapter_dir)))
        verses_html: dict[VerseRef, str] = {}
        for filepath in verse_paths:
            verse_ref = Path(filepath).stem
            # There was a case of a verse file being named '18.txt, so we handle
            # such cases since we need to cast to int below:
            verse_ref = clean_numeric_string(verse_ref)
            with open(filepath, "r") as f:
                verse_md_content = f.read()
                verse_md_content = transform_ta_and_tn_links(
                    verse_md_content,
                    lang_code,
                    resource_requests,
                )
                verse_html_content = cast(str, mistune.markdown(verse_md_content))
                adjusted_verse_html_content = sub(h1, h5, verse_html_content)
                verses_html[verse_ref] = verse_label_fmt_str.format(
                    book_names[book_code],
                    chapter_num,
                    int(verse_ref),
                    adjusted_verse_html_content,
                )
                chapter_verses[chapter_num] = TQChapter(verses=verses_html)
    return chapter_verses


def tq_book_content(
    resource_lookup_dto: ResourceLookupDto,
    resource_dir: str,
    resource_requests: Sequence[ResourceRequest],
    layout_for_print: bool,
) -> TQBook:
    chapter_verses = tq_chapter_verses(
        resource_dir,
        resource_lookup_dto.lang_code,
        resource_lookup_dto.book_code,
        resource_requests,
    )
    return TQBook(
        lang_code=resource_lookup_dto.lang_code,
        lang_name=resource_lookup_dto.lang_name,
        book_code=resource_lookup_dto.book_code,
        resource_type_name=resource_lookup_dto.resource_type_name,
        chapters=chapter_verses,
        lang_direction=resource_lookup_dto.lang_direction,
    )


def tw_sort_key(name_content_pair: TWNameContentPair) -> str:
    return name_content_pair.localized_word


def tw_name_content_pairs(
    resource_dir: str,
    lang_code: str,
    resource_requests: Sequence[ResourceRequest],
    generate_docx: bool,
    h1: str = H1,
    h2: str = H2,
    h3: str = H3,
    h4: str = H4,
) -> list[TWNameContentPair]:
    translation_word_filepaths_ = translation_word_filepaths(resource_dir)
    name_content_pairs = []
    translation_words_dict_ = translation_words_dict(resource_dir)
    for translation_word_filepath in translation_word_filepaths_:
        with open(translation_word_filepath, "r") as f:
            translation_word_content = f.read()
            # French has a single double quote at the start of some
            # translation words which disturbs expected alphabetization,
            # remove it if present. Other languages may have the same defect.
            if translation_word_content.startswith('# "'):
                translation_word_content = f"# {translation_word_content[3:]}"
            localized_translation_word_ = localized_translation_word(
                translation_word_content
            )
            if not localized_translation_word_:  # language doesn't provide data
                continue
            translation_word_content = remove_sections(translation_word_content)
            translation_word_content = transform_ta_and_tn_links(
                translation_word_content, lang_code, resource_requests
            )
            translation_word_content = transform_tw_links(
                translation_word_content,
                lang_code,
                resource_requests,
                translation_words_dict_,
            )
            html_word_content = cast(str, mistune.markdown(translation_word_content))
            html_word_content = sub(h2, h4, html_word_content)
            html_word_content = sub(h1, h3, html_word_content)
            pair = TWNameContentPair(
                localized_translation_word_,
                translation_word_filepath,
                html_word_content,
            )
            # logger.debug("tw_name_content_pair: %s", f"{pair.localized_word}, {pair.path}")
            name_content_pairs.append(pair)
    return sorted(name_content_pairs, key=tw_sort_key)


def tw_book_content(
    resource_lookup_dto: ResourceLookupDto,
    resource_dir: str,
    resource_requests: Sequence[ResourceRequest],
    layout_for_print: bool,
    generate_docx: bool,
) -> TWBook:
    name_content_pairs = tw_name_content_pairs(
        resource_dir, resource_lookup_dto.lang_code, resource_requests, generate_docx
    )
    return TWBook(
        lang_code=resource_lookup_dto.lang_code,
        lang_name=resource_lookup_dto.lang_name,
        resource_type_name=resource_lookup_dto.resource_type_name,
        name_content_pairs=name_content_pairs,
        lang_direction=resource_lookup_dto.lang_direction,
    )


def bc_book_intro_content(
    resource_dir: str,
    book_code: str,
    book_intro_glob_path_fmt_str: str = "{}/*{}/intro.md",
) -> str:
    book_intro_paths = glob(
        book_intro_glob_path_fmt_str.format(resource_dir, book_code)
    )
    if book_intro_paths:
        with open(book_intro_paths[0], "r") as f:
            return f.read()
    else:
        return ""


def modify_commentary_label(
    chapter_commentary_html_content: str, chapter_num: int
) -> str:
    # Modify chapter heading if it's the first chapter
    if chapter_num == 1:
        chapter_commentary_html_content = sub(
            r"<h1>(.*?)<\/h1>",
            r"<h1>\1 Commentary</h1>",
            chapter_commentary_html_content,
        )
    return chapter_commentary_html_content


def replace_relative_with_absolute_links(
    chapter_commentary_html_content: str,
    url_fmt_str: str = BC_ARTICLE_URL_FMT_STR,
) -> str:
    chapter_commentary_html_content = sub(
        r'<a\s+href="\/(.*?)">',
        lambda match: '<a href="'
        + url_fmt_str.format(match.group(1))
        + '" target="_blank">',
        chapter_commentary_html_content,
    )
    return chapter_commentary_html_content


def bc_chapters(
    resource_dir: str,
    lang_code: str,
    book_code: str,
    resource_requests: Sequence[ResourceRequest],
    chapter_dirs_glob_fmt_str: str = "{}/*{}/*[0-9]*.md",
    url_fmt_str: str = BC_ARTICLE_URL_FMT_STR,
) -> dict[int, BCChapter]:
    chapter_dirs = sorted(
        glob(chapter_dirs_glob_fmt_str.format(resource_dir, book_code))
    )
    chapters: dict[int, BCChapter] = {}
    for chapter_dir in chapter_dirs:
        chapter_num = int(Path(chapter_dir).stem)
        with open(chapter_dir, "r") as f:
            chapter_commentary_md_content_raw = f.read()
            chapter_commentary_md_content_cleaned = remove_sections(
                chapter_commentary_md_content_raw
            )
            chapter_commentary_md_content_transformed = transform_ta_and_tn_links(
                chapter_commentary_md_content_cleaned, lang_code, resource_requests
            )
            chapter_commentary_html_content = cast(
                str, mistune.markdown(chapter_commentary_md_content_transformed)
            )
            chapter_commentary_html_content_modified = modify_commentary_label(
                chapter_commentary_html_content, chapter_num
            )
            chapter_commentary_html_content_without_pagination_symbols = (
                remove_pagination_symbols(chapter_commentary_html_content_modified)
            )
            chapter_commentary_html_content_with_absolute_links = (
                replace_relative_with_absolute_links(
                    chapter_commentary_html_content_without_pagination_symbols
                )
            )
            chapter_commentary_html_content_with_adjusted_headings = (
                demote_headings_by_one(
                    chapter_commentary_html_content_with_absolute_links
                )
            )
            # TODO For now we are deactivating the links to articles from commentary. It
            # would be nice to provide those markdown articles as rendered HTML so that the
            # user can follow those links.
            chapter_commentary_html_content = remove_links(
                chapter_commentary_html_content_with_adjusted_headings
            )
            chapters[chapter_num] = BCChapter(
                commentary=chapter_commentary_html_content
            )
    return chapters


def bc_book_content(
    resource_lookup_dto: ResourceLookupDto,
    resource_dir: str,
    resource_requests: Sequence[ResourceRequest],
    layout_for_print: bool,
) -> BCBook:
    book_intro = bc_book_intro_content(resource_dir, resource_lookup_dto.book_code)
    book_intro = remove_sections(book_intro)
    book_intro = transform_ta_and_tn_links(
        book_intro, resource_lookup_dto.lang_code, resource_requests
    )
    book_intro_html_content = cast(str, mistune.markdown(book_intro))
    book_intro_html_content = demote_headings_by_one(book_intro_html_content)
    book_intro_html_content = remove_pagination_symbols(book_intro_html_content)
    book_intro_html_content = remove_links(book_intro_html_content)
    return BCBook(
        book_intro=book_intro_html_content,
        lang_code=resource_lookup_dto.lang_code,
        lang_name=resource_lookup_dto.lang_name,
        book_code=resource_lookup_dto.book_code,
        resource_type_name=resource_lookup_dto.resource_type_name,
        chapters=bc_chapters(
            resource_dir,
            resource_lookup_dto.lang_code,
            resource_lookup_dto.book_code,
            resource_requests,
        ),
    )


def books(
    resource_lookup_dtos: Sequence[ResourceLookupDto],
    resource_dirs: Sequence[str],
    resource_requests: Sequence[ResourceRequest],
    layout_for_print: bool,
    use_chapter_labels: bool,
    generate_docx: bool,
    usfm_resource_types: Sequence[str] = settings.USFM_RESOURCE_TYPES,
    tn_resource_type: str = TN_RESOURCE_TYPE,
    en_tn_condensed_resource_type: str = EN_TN_CONDENSED_RESOURCE_TYPE,
    tq_resource_type: str = TQ_RESOURCE_TYPE,
    tw_resource_type: str = TW_RESOURCE_TYPE,
    bc_resource_type: str = BC_RESOURCE_TYPE,
    rg_resource_type: str = RG_RESOURCE_TYPE,
    docx_file_path: str = "en_rg_nt_survey.docx",
    en_rg_dir: str = settings.EN_RG_DIR,
    book_id_map: dict[str, int] = BOOK_ID_MAP,
) -> tuple[
    Sequence[USFMBook],
    Sequence[TNBook],
    Sequence[TNCBook],
    Sequence[TQBook],
    Sequence[TWBook],
    Sequence[BCBook],
    Sequence[RGBook],
]:
    usfm_books = []
    tn_books = []
    tnc_books = []
    tq_books = []
    tw_books = []
    bc_books = []
    rg_books = []
    filtered_rg_books = []
    for resource_lookup_dto, resource_dir in zip(resource_lookup_dtos, resource_dirs):
        if resource_lookup_dto.resource_type in usfm_resource_types:
            usfm_book = usfm_book_content(
                resource_lookup_dto,
                resource_dir,
                use_chapter_labels,
            )
            usfm_books.append(usfm_book)
        elif resource_lookup_dto.resource_type == tn_resource_type:
            tn_book = tn_book_content(
                resource_lookup_dto, resource_dir, resource_requests, layout_for_print
            )
            tn_books.append(tn_book)
        elif (
            resource_lookup_dto.resource_type == en_tn_condensed_resource_type
        ):  # Handle English Condensed TN
            tnc_book = tnc_book_content(
                resource_lookup_dto, resource_dir, resource_requests, layout_for_print
            )
            tnc_books.append(tnc_book)
        elif resource_lookup_dto.resource_type == tq_resource_type:
            tq_book = tq_book_content(
                resource_lookup_dto, resource_dir, resource_requests, layout_for_print
            )
            tq_books.append(tq_book)
        elif resource_lookup_dto.resource_type == tw_resource_type:
            tw_book = tw_book_content(
                resource_lookup_dto,
                resource_dir,
                resource_requests,
                layout_for_print,
                generate_docx,
            )
            tw_books.append(tw_book)
        elif resource_lookup_dto.resource_type == bc_resource_type:
            bc_book = bc_book_content(
                resource_lookup_dto, resource_dir, resource_requests, layout_for_print
            )
            bc_books.append(bc_book)
        elif resource_lookup_dto.resource_type == rg_resource_type:
            path = join(en_rg_dir, docx_file_path)
            logger.debug("About to get_rg_books from: %s", path)
            rg_books = get_rg_books(
                path,
                resource_lookup_dto.lang_code,
                resource_lookup_dto.lang_name,
                resource_lookup_dto.resource_type_name,
                resource_lookup_dto.lang_direction,
            )
            filtered_rg_books = [
                rg_book
                for rg_book in rg_books
                if rg_book.lang_code == resource_lookup_dto.lang_code
                and rg_book.book_code == resource_lookup_dto.book_code
            ]
    return (
        usfm_books,
        tn_books,
        tnc_books,
        tq_books,
        tw_books,
        bc_books,
        filtered_rg_books,
    )


def ensure_paragraph_before_verses(
    usfm_file: str,
    verse_content: str,
    usfm_verse_one_file_regex: str = r"^01\..*",
    chapter_marker_not_on_own_line_regex: str = r"^\\c [0-9]+ .*|\n",
    chapter_marker_not_on_own_line_with_match_groups: str = r"(^\\c [0-9]+) (.*|\n)",
    chapter_marker_not_on_own_line_repair_regex: str = r"\1\n\n\2\n",
) -> str:
    r"""
    If verse_content has a USFM chapter marker, \c, that is not on its
    own line (violation of the USFM spec) then repair this and
    additionally add a USFM paragraph marker, \p, so that when the USFM is
    rendered to HTML the verse spans will be enclosed in a block level
    HTML element which in turn will ensure that Docx rendering is free of
    a bug wherein the verse spans are interpreted as a continuation of the
    chapter headline (as evidenced by verse content being rendered with
    the same font color and boldness as the chapter headline).
    Return the possibly updated verse_content.
    """
    if (
        compile(usfm_verse_one_file_regex).match(Path(usfm_file).name) is not None
    ):  # Verse 1 of chapter
        if (
            compile(chapter_marker_not_on_own_line_regex).match(verse_content)
            is not None
        ):  # Chapter marker not on own line.
            # Make chapter marker occupy its own line and add a USFM paragraph
            # marker right after it. Why? Because languages which render correctly
            # in Docx have a \p USFM marker after the chapter marker and languages
            # which did not render properly (see docstring above for particulars) in
            # Docx did not have one. Presumably the 3rd party lib we use to parse
            # HTML to Docx doesn't like spans that are not contained in a block
            # level element.
            verse_content = sub(
                chapter_marker_not_on_own_line_with_match_groups,
                chapter_marker_not_on_own_line_repair_regex,
                verse_content,
            )
    return verse_content


def get_book_name(
    resource_path: str,
    resource_lookup_dto: ResourceLookupDto,
    bible_book_names: Mapping[str, str] = BOOK_NAMES,
    use_localized_book_name: bool = settings.USE_LOCALIZED_BOOK_NAME,
) -> str:
    """Retrieve the book name, preferring a localized title if available."""
    title_path = join(resource_path, "front", "title.txt")
    if use_localized_book_name:
        try:
            with open(title_path, encoding="utf-8") as f:
                return normalize_localized_book_name(f.read())
        except FileNotFoundError:
            logger.debug(
                "Localized book name not found, using English book name instead."
            )
    return bible_book_names[resource_lookup_dto.book_code]


def assemble_chapter_usfm(
    chapter_dir: DirEntry[str],
    use_chapter_labels: bool,
) -> list[str]:
    chapter_usfm_content = []
    chapter_num = get_chapter_number(chapter_dir.name)
    chapter_usfm_content.append("\n" + rf"\c {chapter_num}" + "\n")
    if use_chapter_labels:
        chapter_word_file = join(chapter_dir.path, "title.txt")
        chapter_word = read_chapter_label(chapter_word_file)
        if chapter_word is not None:
            chapter_label = "\n" + rf"\cl {chapter_word} {chapter_num}" + "\n"
            chapter_usfm_content.append(chapter_label)
    logger.info(
        "Adding a USFM chapter marker for chapter: %s",
        chapter_num,
    )
    chapter_verse_chunk_files = get_chapter_verse_chunk_files(chapter_dir)
    for usfm_file in chapter_verse_chunk_files:
        verse_content = read_verse_file(usfm_file)
        cleaned_verse_content = clean_verse_content(verse_content)
        verse_content = ensure_paragraph_before_verses(usfm_file, cleaned_verse_content)
        chapter_usfm_content.append(verse_content)
        chapter_usfm_content.append(
            " \n"
        )  # Make sure a space before next chunk, e.g., auh, mat, ch 9, v 14
    return chapter_usfm_content


def get_chapter_number(chapter_dir_name: str) -> int:
    try:
        return int(chapter_dir_name)
    except ValueError:
        logger.info(
            "%s is not a valid chapter number, assigning -1 as chapter number",
            chapter_dir_name,
        )
        return -1  # Sentinel value


def read_chapter_label(chapter_word_file: str) -> Optional[str]:
    try:
        with open(chapter_word_file, "r") as fin:
            chapter_word = fin.read().strip()
            return chapter_label_sans_numeric_part(chapter_word)
    except FileNotFoundError:
        return None


def read_verse_file(usfm_file: str) -> str:
    with open(usfm_file, "r") as fin:
        return fin.read()


def clean_verse_content(verse_content: str) -> str:
    """
    Some languages put a chapter marker in front of verse 1 in the
    verse file which covers a verse span which includes verse 1. Since we
    ensure chapter markers ourselves when assembling multiple verse files
    into a chapter this ends up creating a duplicate chapter marker.
    We deal with that here.
    """
    cleaned_verse_content = sub(r"^\\c\s+\d+", "", verse_content)
    return cleaned_verse_content


def get_chapter_verse_chunk_files(chapter_dir: DirEntry[str]) -> Sequence[str]:
    return sorted(
        [
            file.path
            for file in scandir(chapter_dir)
            if file.is_file()
            and file.name != "title.txt"
            and not file.name.startswith(".")
            and (file.name.endswith(".usfm") or file.name.endswith(".txt"))
        ]
    )


def combine_usfm_files(
    resource_dir: str,
    resource_lookup_dto: ResourceLookupDto,
    use_chapter_labels: bool,
) -> str:
    """
    Attempt to assemble and construct parseable USFM content for USFM
    resource where repo has multiple chapter directories containing verse
    content files.
    """
    logger.info("About to assemble USFM content into a single USFM file.")
    logger.info("Adding a USFM \\ide marker which the parser requires.")
    usfm_content = [r"\ide UTF-8" + "\n"]
    logger.info("Adding a USFM \\h marker which the parser requires.")
    book_name = get_book_name(resource_dir, resource_lookup_dto)
    usfm_content.append(rf"\h {book_name}" + "\n")
    subdirs = [
        file
        for file in scandir(resource_dir)
        if file.is_dir()
        and file.name not in ["front", "00"]
        and not file.name.startswith(".")
    ]
    for chapter_dir in sorted(subdirs, key=lambda dir_entry: dir_entry.name):
        chapter_usfm_content = assemble_chapter_usfm(chapter_dir, use_chapter_labels)
        usfm_content.extend(chapter_usfm_content)
    filename = join(
        resource_dir,
        (
            f"{resource_lookup_dto.lang_code}_"
            f"{resource_lookup_dto.resource_type}_"
            f"{resource_lookup_dto.book_code}.usfm"
        ),
    )
    logger.info("Writing USFM content to: %s", filename)
    with open(filename, "w") as fout:
        fout.write("".join(usfm_content))
    return filename


# Used by STET and PASSAGES apps
def lookup_verse_text(usfm_book: USFMBook, chapter_num: int, verse_ref: str) -> str:
    chapter = usfm_book.chapters.get(chapter_num)
    if not chapter or not chapter.verses:
        return ""
    verse = chapter.verses.get(verse_ref, "")
    logger.info(
        "lang_code: %s, book_code: %s, national_book_name: %s, chapter_num: %s, verse_num: %s, verse: %s",
        usfm_book.lang_code,
        usfm_book.book_code,
        usfm_book.national_book_name,
        chapter_num,
        verse_ref,
        verse,
    )
    return verse


def split_chapter_into_verses_with_formatting(
    chapter: USFMChapter,
) -> dict[VerseRef, str]:
    """
    Parse chapter.content as HTML, extract each <span class="verse">,
    unwrap <span class="word-entry"> elements (preserving their text),
    and return a dict mapping verse number -> cleaned HTML fragment for that verse.

    Sample HTML content with multiple verse elements:

    >>> html_content = '''
    ... <span class="verse">
    ... <sup class="versemarker">1</sup>
    ... <span class="word-entry"> Généalogie </span>
    ... <span class="word-entry">  </span>
    ...  de
    ... <span class="word-entry"> Jésus </span>
    ... -
    ... <span class="word-entry"> Christ </span>
    ... ,
    ... <span class="word-entry"> fils </span>
    ...  de
    ... <span class="word-entry"> David </span>
    ... ,
    ... <span class="word-entry"> fils </span>
    ...  d'
    ... <span class="word-entry"> Abraham </span>
    ... .
    ...
    ... </span>
    ... <span class="verse">
    ... <sup class="versemarker">2</sup>
    ... <span class="word-entry"> Abraham </span>
    ...
    ... <span class="word-entry"> engendra </span>
    ...
    ... <span class="word-entry"> Isaac </span>
    ... ;
    ... <span class="word-entry">  </span>
    ...
    ... <span class="word-entry"> Isaac </span>
    ...
    ... <span class="word-entry"> engendra </span>
    ...
    ... <span class="word-entry"> Jacob </span>
    ... ;
    ... <span class="word-entry">  </span>
    ...
    ... <span class="word-entry"> Jacob </span>
    ...
    ... <span class="word-entry"> engendra </span>
    ...
    ... <span class="word-entry"> Juda </span>
    ...
    ... <span class="word-entry"> et </span>
    ...
    ... <span class="word-entry"> ses </span>
    ...
    ... <span class="word-entry"> frères </span>
    ... ;
    ... </span>
    ... '''
    >>> from doc.domain.parsing import split_chapter_into_verses_with_formatting
    >>> chapter = USFMChapter(content=html_content, verses=None)
    >>> chapter.verses = split_chapter_into_verses_with_formatting(chapter)
    >>> print(chapter.verses["1"])
    <span class="verse"> Généalogie de Jésus-Christ, fils de David, fils d'Abraham. </span>
    """
    soup = BeautifulSoup(chapter.content, "html.parser")
    verse_dict: dict[VerseRef, str] = {}
    for verse_span in soup.find_all("span", class_="verse"):
        sup = verse_span.find("sup", class_="versemarker")
        if not sup or not sup.string:
            continue
        verse_number = sup.string.strip()
        # Remove the versemarker number sup
        sup.decompose()
        # fr f10 uses word-entry tags
        for we in verse_span.find_all("span", class_="word-entry"):
            we.unwrap()
        cleaned_html = clean_content_html(str(verse_span))
        verse_dict[verse_number] = cleaned_html
    return verse_dict


def clean_content_html(raw_content: str) -> str:
    soup = BeautifulSoup(raw_content, "html.parser")
    cleaned_html = str(soup)
    cleaned_html = sub(r"\s+([,;:.!?])", r"\1", cleaned_html)
    cleaned_html = sub(r"\s+'", "'", cleaned_html)
    cleaned_html = sub(r"'\s+", "'", cleaned_html)
    cleaned_html = sub(r"\s*-\s*", "-", cleaned_html)
    cleaned_html = sub(r"\s{2,}", " ", cleaned_html).strip()
    return cleaned_html


if __name__ == "__main__":

    # To run the doctests in this module, in the root of the project do:
    # PYTHONPATH=backend python backend/doc/domain/parsing.py
    # or
    # PYTHONPATH=backend python backend/doc/domain/parsing.py -v
    # These doctests are not collected by pytest: pyproject.toml sets
    # testpaths = ["tests"] with no doctest collection, so run them by hand.
    # See https://docs.python.org/3/library/doctest.html
    # for more details.
    import doctest

    doctest.testmod()
