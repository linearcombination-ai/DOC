"""
This module computes, in a single scan, every (book, resource type)
combination that is available for a language. It backs the UI step
wherein the user chooses resources: a resource is only offered for a
book if the resource really exists for that book.
"""

import re
import threading
from os import scandir
from os.path import isdir
from pathlib import Path
from typing import Mapping, Optional, Sequence

from cachetools import TTLCache
from doc.config import settings
from doc.domain import resource_lookup
from doc.domain.bible_books import (
    BOOK_ID_MAP,
    BOOK_NAMES,
    OLD_TESTAMENT_CODES,
)
from doc.domain.model import (
    BC_RESOURCE_TYPE,
    EN_TN_CONDENSED_RESOURCE_TYPE,
    BookAvailability,
    ResourceAvailability,
    ResourceTypeInfo,
    TN_RESOURCE_TYPE,
    TQ_RESOURCE_TYPE,
    TW_RESOURCE_TYPE,
)
from doc.domain.parsing import find_usfm_files
from doc.utils.url_utils import get_last_segment
from pydantic import HttpUrl

logger = settings.logger(__name__)

SCRIPTURE_CATEGORY = "scripture"
NOTES_CATEGORY = "notes"
QUESTIONS_CATEGORY = "questions"
COMMENTARY_CATEGORY = "commentary"
WORDS_CATEGORY = "words"

BOOK_SCOPE = "book"
LANGUAGE_SCOPE = "language"

# Resource types whose content lives in per book sub directories (or, for tw,
# is not tied to a book at all) rather than in USFM files.
_NOTES_TYPES = (TN_RESOURCE_TYPE, EN_TN_CONDENSED_RESOURCE_TYPE)
_BC_BOOK_DIR_REGEX = re.compile(r"^\d{2,}-([0-9a-z]{3})$")
_TOKEN_SPLIT_REGEX = re.compile(r"[-_.\s/]+")

# Cache of computed availability, per language. Entries live as long as
# the resource assets on disk are considered fresh. Failures (empty
# results) are never cached.
_availability_cache: TTLCache[str, ResourceAvailability] = TTLCache(
    maxsize=256, ttl=settings.ASSET_CACHING_PERIOD * 60
)
_availability_cache_lock = threading.Lock()


def clear_availability_cache() -> None:
    with _availability_cache_lock:
        _availability_cache.clear()


def resource_category(
    resource_type: str,
    usfm_resource_types: Sequence[str] = settings.USFM_RESOURCE_TYPES,
) -> str:
    """
    >>> resource_category("ulb")
    'scripture'
    >>> resource_category("reg")
    'scripture'
    >>> resource_category("tn-condensed")
    'notes'
    >>> resource_category("tw")
    'words'
    """
    if resource_type in usfm_resource_types:
        return SCRIPTURE_CATEGORY
    if resource_type in _NOTES_TYPES:
        return NOTES_CATEGORY
    if resource_type == TQ_RESOURCE_TYPE:
        return QUESTIONS_CATEGORY
    if resource_type == BC_RESOURCE_TYPE:
        return COMMENTARY_CATEGORY
    if resource_type == TW_RESOURCE_TYPE:
        return WORDS_CATEGORY
    return resource_type


def _book_codes_from_usfm_files(
    resource_filepath: str, book_codes: Mapping[str, str] = BOOK_NAMES
) -> set[str]:
    """
    Find the books that a whole language USFM repo contains from the
    names of its USFM files, e.g., 41-MAT.usfm -> mat. USFM files that
    have a txt suffix may live in per book sub directories, so for those
    the path relative to the repo is searched for a book code too.
    """
    found: set[str] = set()
    for usfm_file in find_usfm_files(resource_filepath):
        path = Path(usfm_file)
        if path.suffix.lower() == ".txt":
            try:
                text = str(path.relative_to(resource_filepath))
            except ValueError:
                text = path.name
        else:
            text = path.stem
        tokens = [t for t in _TOKEN_SPLIT_REGEX.split(text.lower()) if t]
        found.update(token for token in tokens if token in book_codes)
    return found


def _book_code_from_repo_name(
    url: HttpUrl,
    lang_code: str,
    zmq_git_username: str = "faustin_azaza",
    book_codes: Mapping[str, str] = BOOK_NAMES,
) -> Optional[str]:
    """
    Per book repos, e.g., ru_gen_text_ulb, carry the book code in their
    name. Return it or None if url is not a per book repo. This mirrors
    the way resource_lookup.resource_lookup_dto identifies such repos.
    """
    last_segment = get_last_segment(url, lang_code)
    repo_components = resource_lookup.update_repo_components(last_segment.split("_"))
    if len(repo_components) > 2:
        book_code = repo_components[1]
        if book_code in book_codes and (
            book_code in str(url) or zmq_git_username in str(url)
        ):
            return book_code
    return None


def scan_repos(
    lang_code: str,
    repo_clone_list: Sequence[tuple[HttpUrl, str, str]],
    usfm_resource_types: Sequence[str] = settings.USFM_RESOURCE_TYPES,
) -> tuple[dict[str, set[str]], set[str]]:
    """
    Walk the (already provisioned) repos of a language and return a
    mapping of book scoped resource type -> set of book codes available,
    along with the set of language wide resource types available.
    """
    book_scoped: dict[str, set[str]] = {}
    language_wide: set[str] = set()
    for url, resource_filepath, resource_type in repo_clone_list:
        if not isdir(resource_filepath):  # Clone/download didn't happen
            continue
        books: set[str] = set()
        if resource_type == TW_RESOURCE_TYPE:
            language_wide.add(resource_type)
            continue
        if resource_type in (TN_RESOURCE_TYPE, TQ_RESOURCE_TYPE) or (
            resource_type == EN_TN_CONDENSED_RESOURCE_TYPE
        ):
            books = {
                entry.name.lower()
                for entry in scandir(resource_filepath)
                if entry.is_dir()
                and not entry.name.startswith(".")
                and entry.name.lower() in BOOK_NAMES
            }
        elif resource_type == BC_RESOURCE_TYPE:
            for entry in scandir(resource_filepath):
                match = _BC_BOOK_DIR_REGEX.match(entry.name.lower())
                if entry.is_dir() and match and match.group(1) in BOOK_NAMES:
                    books.add(match.group(1))
        elif resource_type in usfm_resource_types:
            book_code = _book_code_from_repo_name(url, lang_code)
            if book_code:
                # Per book repo, it must actually have USFM content
                if find_usfm_files(resource_filepath):
                    books = {book_code}
            else:
                books = _book_codes_from_usfm_files(resource_filepath)
        if books:
            book_scoped.setdefault(resource_type, set()).update(books)
    return book_scoped, language_wide


def build_resource_availability(
    lang_code: str,
    book_scoped: Mapping[str, set[str]],
    language_wide: set[str],
    localized_book_names: Mapping[str, str],
    resource_type_names: Mapping[str, str] = settings.RESOURCE_TYPE_CODES_AND_NAMES,
    book_names: Mapping[str, str] = BOOK_NAMES,
) -> ResourceAvailability:
    """
    Assemble the response model. Resource types are ordered by their
    display name, which is the order the document generator has always
    used to order resources within a book, books are in canonical Bible
    order.
    """
    type_codes = sorted(
        {*book_scoped.keys(), *language_wide},
        key=lambda code: resource_type_names.get(code, code),
    )
    resource_types = [
        ResourceTypeInfo(
            code=code,
            name=resource_type_names.get(code, code),
            category=resource_category(code),
            scope=LANGUAGE_SCOPE if code in language_wide else BOOK_SCOPE,
        )
        for code in type_codes
    ]
    books_by_code: dict[str, list[str]] = {}
    for code in type_codes:
        if code in language_wide:
            continue
        for book_code in book_scoped.get(code, ()):
            books_by_code.setdefault(book_code, []).append(code)
    books = [
        BookAvailability(
            code=book_code,
            name=localized_book_names.get(book_code) or book_names[book_code],
            testament="OT" if book_code in OLD_TESTAMENT_CODES else "NT",
            resources=resources,
        )
        for book_code, resources in sorted(
            books_by_code.items(), key=lambda item: BOOK_ID_MAP[item[0]]
        )
    ]
    return ResourceAvailability(
        lang_code=lang_code,
        resource_types=resource_types,
        books=books,
        language_resources=sorted(language_wide),
    )


def resource_availability(
    lang_code: str,
    asset_caching_enabled: bool = settings.ASSET_CACHING_ENABLED,
    download_assets: bool = settings.DOWNLOAD_ASSETS,
) -> ResourceAvailability:
    """
    Return every (book, resource type) combination available for
    lang_code, computed in one pass over the language's repos.
    """
    if asset_caching_enabled:
        with _availability_cache_lock:
            cached_availability = _availability_cache.get(lang_code)
        if cached_availability is not None:
            return cached_availability
    empty = ResourceAvailability(
        lang_code=lang_code, resource_types=[], books=[], language_resources=[]
    )
    data = resource_lookup.fetch_source_data()
    if data is None or not data.git_repo:
        logger.info("Data API is down or no git_repo found!")
        return empty
    try:
        repos_info = resource_lookup.filter_excluded_resource_types(data.git_repo)
        augmented_repos_info = resource_lookup.add_data_not_supplied_by_data_api(
            repos_info
        )
        repo_clone_list = resource_lookup.repos_to_clone(
            lang_code, augmented_repos_info
        )
        repos_to_acquire = [
            (url, path)
            for url, path, resource_type in repo_clone_list
            if resource_type != "rg"
        ]
        if download_assets:
            resource_lookup.batch_download_repos(repos_to_acquire)
        else:
            resource_lookup.batch_clone_git_repos(repos_to_acquire)
        book_scoped, language_wide = scan_repos(lang_code, repo_clone_list)
        try:
            localized_book_names = dict(
                resource_lookup.get_book_codes_for_lang_(
                    repo_clone_list, lang_code, False
                )
            )
        except Exception:
            logger.exception("Couldn't get localized book names, using English.")
            localized_book_names = {}
        availability = build_resource_availability(
            lang_code, book_scoped, language_wide, localized_book_names
        )
    except Exception:
        logger.exception("Failed due to the following exception.")
        return empty
    if asset_caching_enabled and (availability.books or availability.resource_types):
        with _availability_cache_lock:
            _availability_cache[lang_code] = availability
    return availability
