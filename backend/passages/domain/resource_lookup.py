from passages.reviewers_guide.model import BibleReference
from passages.domain.survey_files import SURVEY_FILES, en_rg_dir
from passages.reviewers_guide.parser import get_rg_books
from os.path import exists, isdir, join
from doc.domain.resource_lookup import book_codes_for_lang_from_usfm_only
from doc.domain.model import (
    NON_USFM_RESOURCE_TYPES,
    Content,
    LangDirEnum,
    Language,
    RepoEntry,
    ResourceLookupDto,
    SourceData,
)


def nt_survey_rg_passages(
    lang_code: str = "en",
    lang_name: str = "English",
    docx_file_path: str = SURVEY_FILES["nt"],
    resource_type_name: str = "NT Survey Reviewers' Guide",
    lang_direction: LangDirEnum = LangDirEnum.LTR,
) -> list[BibleReference]:
    """
    Returns the list of all NT RG passages from the docx_file_path, but with
    book names localized for language chosen.

    >>> from passages.domain import resource_lookup
    >>> ();rg_books = resource_lookup.nt_survey_rg_passages() ;() # doctest: +ELLIPSIS
    (...)
    >>> rg_books[0]
    BibleReference(lang_code='en', book_code='mat', book_name='Matthew', start_chapter=2, start_chapter_verse_ref='1-12', end_chapter=None, end_chapter_verse_ref=None)
    """
    path = join(en_rg_dir(), docx_file_path)
    rg_books = get_rg_books(
        path,
        lang_code,
        lang_name,
        resource_type_name,
        lang_direction,
    )
    rg_book_chapters = [
        chapter for rg_book in rg_books for chapter in rg_book.chapters.values()
    ]
    bible_references = [
        pt.bible_reference for chapter in rg_book_chapters for pt in chapter.content
    ]
    # Localize the book names since they are provided in English from the survey .docx
    book_name_map = {
        book_code_and_name[0]: book_code_and_name[1]
        for book_code_and_name in book_codes_for_lang_from_usfm_only(lang_code)
    }
    for bible_reference in bible_references:
        maybe_localized_book_name = book_name_map.get(
            bible_reference.book_code, bible_reference.book_name
        )
        bible_reference.lang_code = lang_code
        bible_reference.book_name = maybe_localized_book_name
    return bible_references


def ot_survey_rg1_passages(
    lang_code: str = "en",
    lang_name: str = "English",
    docx_file_path: str = SURVEY_FILES["ot_rg1"],
    resource_type_name: str = "OT Survey Reviewers' Guide (Genesis to Deuteronomy)",
    lang_direction: LangDirEnum = LangDirEnum.LTR,
) -> list[BibleReference]:
    """
    >>> from doc.domain import resource_lookup
    >>> ();rg_books = resource_lookup.ot_survey_rg1_passages();() # doctest: +ELLIPSIS
    (...)
    >>> rg_books[0]
    BibleReference(lang_code='en', book_code='gen', book_name='Genesis', start_chapter=1, start_chapter_verse_ref='1', end_chapter=2, end_chapter_verse_ref='3')
    """
    path = join(en_rg_dir(), docx_file_path)
    rg_books = get_rg_books(
        path,
        lang_code,
        lang_name,
        resource_type_name,
        lang_direction,
    )
    rg_book_chapters = [
        chapter for rg_book in rg_books for chapter in rg_book.chapters.values()
    ]
    bible_references = [
        pt.bible_reference for chapter in rg_book_chapters for pt in chapter.content
    ]
    # Localize the book names since they are provided in English from the survey .docx
    book_name_map = {
        book_code_and_name[0]: book_code_and_name[1]
        for book_code_and_name in book_codes_for_lang_from_usfm_only(lang_code)
    }
    for bible_reference in bible_references:
        maybe_localized_book_name = book_name_map.get(
            bible_reference.book_code, bible_reference.book_name
        )
        bible_reference.lang_code = lang_code
        bible_reference.book_name = maybe_localized_book_name
    return bible_references


def ot_survey_rg2_passages(
    lang_code: str = "en",
    lang_name: str = "English",
    docx_file_path: str = SURVEY_FILES["ot_rg2"],
    resource_type_name: str = "OT Survey Reviewers' Guide (Joshua to Esther)",
    lang_direction: LangDirEnum = LangDirEnum.LTR,
) -> list[BibleReference]:
    """
    >>> from doc.domain import resource_lookup
    >>> ();rg_books = resource_lookup.ot_survey_rg2_passages();() # doctest: +ELLIPSIS
    (...)
    >>> rg_books[0]
    BibleReference(lang_code='en', book_code='jos', book_name='Joshua', start_chapter=1, start_chapter_verse_ref='1-9', end_chapter=None, end_chapter_verse_ref=None)
    """
    path = join(en_rg_dir(), docx_file_path)
    rg_books = get_rg_books(
        path,
        lang_code,
        lang_name,
        resource_type_name,
        lang_direction,
    )
    rg_book_chapters = [
        chapter for rg_book in rg_books for chapter in rg_book.chapters.values()
    ]
    bible_references = [
        pt.bible_reference for chapter in rg_book_chapters for pt in chapter.content
    ]
    # Localize the book names since they are provided in English from the survey .docx
    book_name_map = {
        book_code_and_name[0]: book_code_and_name[1]
        for book_code_and_name in book_codes_for_lang_from_usfm_only(lang_code)
    }
    for bible_reference in bible_references:
        maybe_localized_book_name = book_name_map.get(
            bible_reference.book_code, bible_reference.book_name
        )
        bible_reference.lang_code = lang_code
        bible_reference.book_name = maybe_localized_book_name
    return bible_references


def ot_survey_rg3_passages(
    lang_code: str = "en",
    lang_name: str = "English",
    docx_file_path: str = SURVEY_FILES["ot_rg3"],
    resource_type_name: str = "OT Survey Reviewers' Guide (Job to Song of Songs)",
    lang_direction: LangDirEnum = LangDirEnum.LTR,
) -> list[BibleReference]:
    """
    >>> from doc.domain import resource_lookup
    >>> ();rg_books = resource_lookup.ot_survey_rg3_passages();() # doctest: +ELLIPSIS
    (...)
    >>> rg_books[0]
    BibleReference(lang_code='en', book_code='job', book_name='Job', start_chapter=1, start_chapter_verse_ref='6-22', end_chapter=None, end_chapter_verse_ref=None)
    """
    path = join(en_rg_dir(), docx_file_path)
    rg_books = get_rg_books(
        path,
        lang_code,
        lang_name,
        resource_type_name,
        lang_direction,
    )
    rg_book_chapters = [
        chapter for rg_book in rg_books for chapter in rg_book.chapters.values()
    ]
    bible_references = [
        pt.bible_reference for chapter in rg_book_chapters for pt in chapter.content
    ]
    # Localize the book names since they are provided in English from the survey .docx
    book_name_map = {
        book_code_and_name[0]: book_code_and_name[1]
        for book_code_and_name in book_codes_for_lang_from_usfm_only(lang_code)
    }
    for bible_reference in bible_references:
        maybe_localized_book_name = book_name_map.get(
            bible_reference.book_code, bible_reference.book_name
        )
        bible_reference.lang_code = lang_code
        bible_reference.book_name = maybe_localized_book_name
    return bible_references


def ot_survey_rg4_passages(
    lang_code: str = "en",
    lang_name: str = "English",
    docx_file_path: str = SURVEY_FILES["ot_rg4"],
    resource_type_name: str = "OT Survey Reviewers' Guide (Isaiah to Malachi)",
    lang_direction: LangDirEnum = LangDirEnum.LTR,
) -> list[BibleReference]:
    """
    >>> from doc.domain import resource_lookup
    >>> ();rg_books = resource_lookup.ot_survey_rg4_passages();() # doctest: +ELLIPSIS
    (...)
    >>> rg_books[0]
    BibleReference(lang_code='en', book_code='isa', book_name='Isaiah', start_chapter=1, start_chapter_verse_ref='1-9', end_chapter=None, end_chapter_verse_ref=None)
    """
    path = join(en_rg_dir(), docx_file_path)
    rg_books = get_rg_books(
        path,
        lang_code,
        lang_name,
        resource_type_name,
        lang_direction,
    )
    rg_book_chapters = [
        chapter for rg_book in rg_books for chapter in rg_book.chapters.values()
    ]
    bible_references = [
        pt.bible_reference for chapter in rg_book_chapters for pt in chapter.content
    ]
    # Localize the book names since they are provided in English from the survey .docx
    book_name_map = {
        book_code_and_name[0]: book_code_and_name[1]
        for book_code_and_name in book_codes_for_lang_from_usfm_only(lang_code)
    }
    for bible_reference in bible_references:
        maybe_localized_book_name = book_name_map.get(
            bible_reference.book_code, bible_reference.book_name
        )
        bible_reference.lang_code = lang_code
        bible_reference.book_name = maybe_localized_book_name
    return bible_references
