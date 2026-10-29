"""
Useful functions that are not class or instance specific for TW
resources that we use in multiple places.
"""

import re
import time
from glob import glob
from os.path import basename
from pathlib import Path
from typing import Mapping, Optional, Sequence

from doc.config import settings
from doc.domain import resource_lookup, bible_books
from doc.domain.model import (
    DocumentPart,
    LangDirEnum,
    ResourceRequest,
    TWBook,
    TWNameContentPair,
    TWUse,
    USFMBook,
)
from doc.utils.list_utils import unique_list_of_strings

logger = settings.logger(__name__)

TW = "tw"


def translation_word_filepaths(resource_dir: str) -> list[str]:
    """
    Get the file paths to the translation word files located
    recursively in resource_dir.
    """
    filepaths = glob("{}/bible/kt/*.md".format(resource_dir))
    filepaths.extend(glob("{}/bible/names/*.md".format(resource_dir)))
    filepaths.extend(glob("{}/bible/other/*.md".format(resource_dir)))
    return filepaths


def localized_translation_word(
    translation_word_content: str,
) -> str:
    """
    Get the localized translation word from the
    translation_word_content.

    Sometimes a translation word file has as its first header a list
    of various forms of the word. If that is the case we use the first
    form of the word in the list.
    """
    first_line = translation_word_content.split("\n")[0]
    first_line_components = first_line.split("# ")
    localized_translation_word = (
        first_line_components[1] if len(first_line_components) >= 2 else ""
    )
    if "," in localized_translation_word:
        # logger.debug(
        #     "localized_translation_word: %s", localized_translation_word
        # )
        # In this case, the localized word is actually multiple forms of the
        # word separated by commas, use the first form of the word.
        localized_translation_word = localized_translation_word.split(",")[0]
        # logger.debug(
        #     "Updated localized_translation_word: %s", localized_translation_word
        # )
    localized_translation_word = str.strip(localized_translation_word)
    return localized_translation_word


def tw_resource_dir(lang_code: str) -> Optional[str]:
    """
    Return the location of the TW resource asset directory given the
    lang_code of the language under consideration. The location is
    based on an established convention for the directory structure to
    be consistent across lang_code, resource_type, and book_code
    combinations.
    """
    # This is a bit hacky to "know" how to derive the actual directory path
    # file pattern/convention to expect and use it literally. But, Being
    # able to derive the tw_resource_dir location from only a lang_code a
    # constant, TW, and a convention allows us to decouple TWResource from
    # other Resource subclass instances. They'd be coupled if we had
    # to pass the value of TWResource's resource_dir to Resource
    # subclasses otherwise. It is a design tradeoff.
    tw_resource_dir_candidates = glob(
        "{}/{}_{}*/{}_{}*".format(
            settings.RESOURCE_ASSETS_DIR, lang_code, TW, lang_code, TW
        )
    )
    # If tw_resource_dir_candidates is empty it is because the user
    # did not request a TW resource as part of their document request
    # which is a valid state of affairs of course. We return the empty
    # string in such cases.
    return tw_resource_dir_candidates[0] if tw_resource_dir_candidates else None


# Some document requests don't include a resource request for
# translation words. In such cases there wouldn't be a tw_resource_dir
# associated with the request (though there could be the actual TW
# resource asset files on disk from a previous document request - we
# wouldn't make the assumption that such files were there however)
# therefore we can't require tw_resource_dir as a precondition.
def translation_words_dict(tw_resource_dir: Optional[str]) -> dict[str, str]:
    """
    Given the path to the TW resource asset files, return a dictionary
    of translation word to translation word filepath mappings,
    otherwise return an empty dictionary.
    """
    translation_words_dict: dict[str, str] = {}
    if tw_resource_dir is not None:
        filepaths = translation_word_filepaths(tw_resource_dir)
        translation_words_dict = {
            Path(basename(word_filepath)).stem: word_filepath
            for word_filepath in filepaths
        }
    return translation_words_dict


def translation_words_section_for_book(
    tw_book: TWBook,
    usfm_books: Optional[Sequence[USFMBook]],
    limit_words: bool,
    resource_requests: Sequence[ResourceRequest],
    include_uses_section: bool = True,
    resource_type_name_fmt_str: str = "<h2>{}</h2>",
) -> Sequence[str]:
    """
    Build and return the translation words definition section, i.e.,
    the list of all translation words for this language, book combination.
    Limit the translation words to only those that appear in the USFM
    resource chosen if limit_words is True and a USFM resource was also
    chosen otherwise include all the translation words for the language.
    """
    content = []
    if tw_book.name_content_pairs:
        lang_codes = list(
            dict.fromkeys(
                resource_request.lang_code for resource_request in resource_requests
            )
        )
        if len(lang_codes) > 1:
            # More than one language was requested so we should
            # differentiate translation word sections by adding the IETF code to the
            # resource type name used as a header.
            heading = resource_type_name_fmt_str.format(
                f"{tw_book.resource_type_name} ({tw_book.lang_code})"
            )
        else:
            heading = resource_type_name_fmt_str.format(tw_book.resource_type_name)
        content.append(heading)
    selected_name_content_pairs = get_selected_name_content_pairs_for_book(
        tw_book, usfm_books, limit_words, resource_requests
    )
    for name_content_pair in selected_name_content_pairs:
        content.append(
            name_content_pair_content_for_book(
                name_content_pair, tw_book, include_uses_section
            )
        )
    return content


def translation_words_for_content(
    tw_book: TWBook,
    content: str,
    resource_type_name_fmt_str: str = "<h2>{}</h2>",
) -> Sequence[tuple[str, str]]:
    selected_name_content_pairs = get_selected_name_content_pairs_for_content(
        tw_book, content
    )
    return [
        (pair.localized_word, Path(pair.path).stem)
        for pair in selected_name_content_pairs
    ]


def translation_words_content(
    tw_book: TWBook,
    content: str,
    use_section_visual_separator: bool,
    link_rather_than_include_tw_definitions: bool = settings.LINK_RATHER_THAN_INCLUDE_TW_DEFINITIONS,
    resource_type_name_fmt_str: str = settings.RESOURCE_TYPE_NAME_FMT_STR,
    biel_tw_resource_path_fmt_str: str = settings.BIEL_TW_RESOURCE_URL_FMT_STR,
    tw_resource_path_fmt_str: str = settings.TW_RESOURCE_URL_FMT_STR,
    div_open: str = "<div>",
    div_close: str = "</div>",
) -> list[DocumentPart]:
    is_rtl = tw_book and tw_book.lang_direction == LangDirEnum.RTL
    document_parts: list[DocumentPart] = []
    words = translation_words_for_content(tw_book, content)
    unique_words = unique_list_of_strings(words)
    if unique_words:
        document_parts.append(
            DocumentPart(
                content=resource_type_name_fmt_str.format(tw_book.resource_type_name),
                is_rtl=is_rtl,
                use_section_visual_separator=False,
            )
        )
        if link_rather_than_include_tw_definitions:
            document_parts.append(
                DocumentPart(
                    content=div_open
                    + ", ".join(
                        [
                            biel_tw_resource_path_fmt_str.format(
                                tw_book.lang_code, localized_word
                            )
                            for localized_word, word in unique_words
                        ]
                    )
                    + div_close,
                    is_rtl=is_rtl,
                    use_section_visual_separator=False,
                )
            )
        else:
            document_parts.append(
                DocumentPart(
                    content=div_open
                    + ", ".join(
                        [
                            tw_resource_path_fmt_str.format(
                                tw_book.lang_code, word, localized_word
                            )
                            for localized_word, word in unique_words
                        ]
                    )
                    + div_close,
                    is_rtl=is_rtl,
                    use_section_visual_separator=False,
                )
            )
    return document_parts


def get_selected_name_content_pairs_for_book(
    tw_book: TWBook,
    usfm_books: Optional[Sequence[USFMBook]],
    limit_words: bool,
    resource_requests: Sequence[ResourceRequest],
) -> list[TWNameContentPair]:
    selected_name_content_pairs = []
    if usfm_books and limit_words:
        selected_name_content_pairs = filter_name_content_pairs_for_book(
            tw_book, usfm_books
        )
    elif (
        not usfm_books and limit_words
    ):  # This branch is necessarily expensive computationally and in IO
        t0 = time.time()
        # USFM resource was not requested by user, but they chose resources that
        # are still associated with books, e.g., TN, so we have to still fetch
        # USFM if limit_words was chosen to see what words appear in USFM so
        # that we can limit the words to that collection of words rather than
        # all of them.
        usfm_books = fetch_usfm_books(resource_requests)
        selected_name_content_pairs = filter_name_content_pairs_for_book(
            tw_book, usfm_books
        )
        t1 = time.time()
        logger.info(
            "Time for acquiring and filtering TW content based on books chosen: %s",
            t1 - t0,
        )
    else:
        selected_name_content_pairs = tw_book.name_content_pairs
    return selected_name_content_pairs


def get_selected_name_content_pairs_for_content(
    tw_book: TWBook,
    content: str,
) -> list[TWNameContentPair]:
    selected_name_content_pairs = []
    selected_name_content_pairs = filter_name_content_pairs_for_content(
        tw_book, content
    )
    return selected_name_content_pairs


def filter_name_content_pairs_for_book(
    tw_book: TWBook, usfm_books: Sequence[USFMBook]
) -> list[TWNameContentPair]:
    selected_name_content_pairs = []
    added_pairs = set()
    for name_content_pair in tw_book.name_content_pairs:
        for usfm_book in usfm_books:
            for chapter in usfm_book.chapters.values():
                if re.search(
                    re.escape(name_content_pair.localized_word),
                    chapter.content,
                ):
                    if name_content_pair not in added_pairs:
                        selected_name_content_pairs.append(name_content_pair)
                        added_pairs.add(name_content_pair)
                    break
    return selected_name_content_pairs


def filter_name_content_pairs_for_content(
    tw_book: TWBook, content: str
) -> list[TWNameContentPair]:
    selected_name_content_pairs = []
    added_pairs = set()
    for name_content_pair in tw_book.name_content_pairs:
        if re.search(
            r"\b" + re.escape(name_content_pair.localized_word) + r"\b",
            content,
            flags=re.IGNORECASE,
        ):
            if name_content_pair not in added_pairs:
                selected_name_content_pairs.append(name_content_pair)
                added_pairs.add(name_content_pair)
    return selected_name_content_pairs


def contains_tw(resource_request: ResourceRequest, tw_regex: str = "tw.*") -> bool:
    """Return True if the resource_request describes a TW resource."""
    value = bool(re.compile(tw_regex).match(resource_request.resource_type))
    return value


def name_content_pair_content_for_book(
    name_content_pair: TWNameContentPair,
    tw_book: TWBook,
    include_uses_section: bool = False,
) -> str:
    name_content_pair.content = modify_content_for_anchors(name_content_pair, tw_book)
    uses_section_ = ""
    # TODO tw_book.uses is no longer populated - look at git history to see how I used to do it
    if include_uses_section and name_content_pair.localized_word in tw_book.uses:
        uses_section_ = uses_section(tw_book.uses[name_content_pair.localized_word])
        name_content_pair.content = f"{name_content_pair.content}{uses_section_}"
    return name_content_pair.content


def filter_unique_by_lang_code(tw_books: Sequence[TWBook]) -> list[TWBook]:
    unique_tw_books = []
    seen_lang_codes = set()
    for tw_book in tw_books:
        lang_code = tw_book.lang_code
        if lang_code not in seen_lang_codes:
            seen_lang_codes.add(lang_code)
            unique_tw_books.append(tw_book)
    return unique_tw_books


def modify_content_for_anchors(
    name_content_pair: TWNameContentPair,
    book_content_unit: TWBook,
    opening_h3_fmt_str: str = "<h3>{}",
    opening_h3_with_id_fmt_str: str = '<h3 id="{}-{}">{}',
) -> str:
    content = name_content_pair.content.replace(
        opening_h3_fmt_str.format(name_content_pair.localized_word),
        opening_h3_with_id_fmt_str.format(
            book_content_unit.lang_code,
            Path(name_content_pair.path).stem,
            name_content_pair.localized_word,
        ),
    )
    return content


def fetch_usfm_books(
    resource_requests: Sequence[ResourceRequest],
    usfm_resource_types: Sequence[str] = settings.USFM_RESOURCE_TYPES,
) -> list[USFMBook]:
    from doc.domain.parsing import usfm_book_content

    usfm_resource_lookup_dtos = []
    for resource_request in resource_requests:
        for usfm_type in usfm_resource_types:
            resource_lookup_dto = resource_lookup.resource_lookup_dto(
                resource_request.lang_code,
                usfm_type,
                resource_request.book_code,
            )
            if resource_lookup_dto:
                usfm_resource_lookup_dtos.append(resource_lookup_dto)
    # Determine which resource URLs were actually found.
    found_usfm_resource_lookup_dtos = [
        resource_lookup_dto
        for resource_lookup_dto in usfm_resource_lookup_dtos
        if resource_lookup_dto.url is not None
    ]
    t0 = time.time()
    resource_dirs = [
        resource_lookup.prepare_resource_filepath(dto)
        for dto in found_usfm_resource_lookup_dtos
    ]
    for resource_dir, dto in zip(resource_dirs, found_usfm_resource_lookup_dtos):
        resource_lookup.provision_asset_files(dto.url, resource_dir)
    t1 = time.time()
    logger.debug(
        "Time to provision USFM asset files (acquire and write to disk) for TW resource: %s",
        t1 - t0,
    )
    # Initialize found resources from their provisioned assets.
    usfm_book_content_units = [
        usfm_book_content(resource_lookup_dto, resource_dir, False)
        for resource_lookup_dto, resource_dir in zip(
            found_usfm_resource_lookup_dtos, resource_dirs
        )
    ]
    return usfm_book_content_units


def fetch_usfm_book(
    lang_code: str,
    book_code: str,
    resource_type: str,
) -> Optional[USFMBook]:
    from doc.domain.parsing import usfm_book_content

    usfm_book = None
    resource_lookup_dto = resource_lookup.resource_lookup_dto(
        lang_code,
        resource_type,
        book_code,
    )
    if resource_lookup_dto and resource_lookup_dto.url:
        t0 = time.time()
        resource_dir = resource_lookup.prepare_resource_filepath(resource_lookup_dto)
        resource_lookup.provision_asset_files(resource_lookup_dto.url, resource_dir)
        t1 = time.time()
        logger.debug(
            "Time to provision USFM asset files (acquire and write to disk) for TW resource: %s",
            t1 - t0,
        )
        usfm_book = usfm_book_content(resource_lookup_dto, resource_dir, False)
    return usfm_book


def uses_section(
    uses: Sequence[TWUse],
    translation_word_verse_section_header_str: str = settings.TRANSLATION_WORD_VERSE_SECTION_HEADER_STR,
    unordered_list_begin_str: str = settings.UNORDERED_LIST_BEGIN_STR,
    translation_word_verse_ref_item_fmt_str: str = settings.TRANSLATION_WORD_VERSE_REF_ITEM_FMT_STR,
    unordered_list_end_str: str = settings.UNORDERED_LIST_END_STR,
    book_numbers: Mapping[str, str] = bible_books.BOOK_NUMBERS,
    book_names: Mapping[str, str] = bible_books.BOOK_NAMES,
    num_zeros: int = 3,
) -> str:
    """
    Construct and return the 'Uses:' section which comes at the end of
    a translation word definition and wherein each item points to
    verses (as targeted by lang_code, book_code, chapter_num, and
    verse_num) wherein the word occurs.
    """
    html: list[str] = []
    html.append(translation_word_verse_section_header_str)
    html.append(unordered_list_begin_str)
    for use in uses:
        html_content_str = translation_word_verse_ref_item_fmt_str.format(
            use.lang_code,
            book_numbers[use.book_id].zfill(num_zeros),
            str(use.chapter_num).zfill(num_zeros),
            str(use.verse_num).zfill(num_zeros),
            book_names[use.book_id],
            use.chapter_num,
            use.verse_num,
        )
        html.append(html_content_str)
    html.append(unordered_list_end_str)
    return "\n".join(html)
