"""
This module provides classes that are used as data transfer objects.
In particular, many of the classes here are subclasses of
pydantic.BaseModel as FastAPI can use these classes to do automatic
validation and JSON serialization.
"""

from dataclasses import dataclass
from enum import Enum
from typing import Annotated, NamedTuple, Optional, Sequence, TypedDict, final

from doc.config import settings
from doc.domain.bible_books import BOOK_NAMES
from doc.utils.number_utils import is_even
from pydantic import BaseModel, EmailStr, HttpUrl
from pydantic.functional_validators import model_validator

# These type aliases give us more self-documenting code, but of course
# aren't strictly necessary.
VerseRef = str
ChapterNum = int

TN_RESOURCE_TYPE: str = "tn"
EN_TN_CONDENSED_RESOURCE_TYPE: str = "tn-condensed"
TQ_RESOURCE_TYPE: str = "tq"
TW_RESOURCE_TYPE: str = "tw"
BC_RESOURCE_TYPE: str = "bc"
NON_USFM_RESOURCE_TYPES: Sequence[str] = [
    TN_RESOURCE_TYPE,
    EN_TN_CONDENSED_RESOURCE_TYPE,
    TQ_RESOURCE_TYPE,
    TW_RESOURCE_TYPE,
    BC_RESOURCE_TYPE,
]


@final
class AssemblyStrategyEnum(str, Enum):
    INTERLEAVE_BY_BOOK = "lbo"
    INTERLEAVE_BY_CHAPTER = "blo"
    INTERLEAVE_BY_VERSE_BOOK_AT_A_TIME = "lvo"  # Interleave by verse one book at a time
    INTERLEAVE_BY_VERSE = "bvo"  # Interleave by verse one chapter at a time
    STET_STRATEGY = "stet"


@final
class AssemblyLayoutEnum(str, Enum):
    """
    An enum used by the assembly_strategies module to know how
    to layout the content.

    We can have N such layouts and each can be completely
    arbitrary, simply based on the desires of content designers.

    Layouts:

    * ONE_COLUMN
      All content in one column

    * ONE_COLUMN_COMPACT
      This layout minimizes whitespace in a one column layout so as to
      be appropriate for printing to paper.

    * TWO_COLUMN_SCRIPTURE_LEFT_SCRIPTURE_RIGHT
      Two columns, with scripture on the left and a different
      scripture on the right. Obviously only applicable when at least
      two languages have been chosen.

    * TWO_COLUMN_SCRIPTURE_LEFT_SCRIPTURE_RIGHT_COMPACT
      This layout minimizes whitespace by using
      TWO_COLUMN_SCRIPTURE_LEFT_SCRIPTURE_RIGHT layout but with a
      different CSS styling that results in less whitespace.
    """

    ONE_COLUMN = "1c"
    ONE_COLUMN_COMPACT = "1c_c"
    # NOTE The next two layouts only make sense
    # with an AssemblyStrategyEnum.BOOK_LANGUAGE_ORDER assembly
    # strategy and when more than one language is chosen for the same
    # book.
    # fmt: off
    TWO_COLUMN_SCRIPTURE_LEFT_SCRIPTURE_RIGHT = "2c_sl_sr"
    TWO_COLUMN_SCRIPTURE_LEFT_SCRIPTURE_RIGHT_COMPACT = "2c_sl_sr_c"
    # fmt: on
    STET_LAYOUT = "stet"


# https://blog.meadsteve.dev/programming/2020/02/10/types-at-the-edges-in-python/
# https://pydantic-docs.helpmanual.io/usage/models/
@final
class ResourceRequest(BaseModel):
    """
    This class is used to encode a request for a resource. A
    document request composes N of these resource request instances.
    """

    lang_code: str
    resource_type: str
    book_code: str


class PassageRequest(BaseModel):
    """
    This class is used to encode a request for a bible passage.
    """

    lang_code: str
    resource_type: str
    book_code: str
    start_chapter_num: ChapterNum
    start_chapter_verse_ref: VerseRef
    end_chapter_num: Optional[ChapterNum]
    end_chapter_verse_ref: Optional[VerseRef]


@final
class DocumentRequestSourceEnum(str, Enum):
    """
    This class/enum captures the concept of: where did the document
    request originate from?
    """

    UI = "ui"
    TEST = "test"
    BIEL_UI = "biel_ui"


@final
@dataclass(frozen=True)
class AffectsOutput:
    """
    Marks a request field that changes the content or formatting of the
    generated files. Use as Annotated[T, AffectsOutput()].
    """


@final
@dataclass(frozen=True)
class DeliveryOnly:
    """
    Marks a request field that does not change the generated files: it
    only selects which of the files get produced or delivered, or only
    influences another field that does affect the output.
    Use as Annotated[T, DeliveryOnly()].
    """


@final
class DocumentRequest(BaseModel):
    """
    This class reifies a document generation request from a client of
    the API.
    """

    email_address: Annotated[Optional[EmailStr], DeliveryOnly()] = None
    assembly_strategy_kind: Annotated[AssemblyStrategyEnum, AffectsOutput()]
    # NOTE For testing we want to exercise various layouts, thus we
    # make this attribute Optional so that we can specify it in unit
    # tests if desired. But the normal case is to set
    # assembly_layout_kind to None in a document request if we are
    # manually coding up a document request instance to send to the
    # API. If you go this latter route, say to use the API in a
    # different context than interacting with the UI then the onus is
    # on you as the developer to choose an assembly_layout_kind that
    # makes sense given the document request you have instantiated.
    # The system knows which make sense and which do not given your
    # document request and if you choose one that does not make sense
    # then you'll get an exception on purpose.
    assembly_layout_kind: Annotated[Optional[AssemblyLayoutEnum], AffectsOutput()] = (
        AssemblyLayoutEnum.ONE_COLUMN
    )
    # The user can choose whether the result should be formatted to
    # print. When the user selects yes/True to format for print
    # then we'll choose a compact layout that makes sense for their
    # document request.
    layout_for_print: Annotated[bool, AffectsOutput()] = False
    resource_requests: Annotated[Sequence[ResourceRequest], AffectsOutput()]
    # Indicate whether PDF should be generated.
    generate_pdf: Annotated[bool, DeliveryOnly()] = True
    # Indicate whether ePub should be generated.
    generate_epub: Annotated[bool, DeliveryOnly()] = False
    # Indicate whether Docx should be generated.
    generate_docx: Annotated[bool, DeliveryOnly()] = False
    # Indicate whether translation words, TW, should be limited to
    # only those that appear in the USFM requested (True), or, include all
    # the TW words available for the language requested (False).
    limit_words: Annotated[bool, AffectsOutput()] = True
    # Indicate whether to use chapter labels at all. False means only use
    # chapter number at start of chapter. True means use chapter label plus
    # chapter number, e.g., Chapter 1. Note that if localize_chapter_labels
    # is True, then the chapter label will be localized to the language(s)
    # requested.
    use_chapter_labels: Annotated[bool, AffectsOutput()] = False
    # We have two PDF processors that we can use: Weasyprint and Princexml.
    # Weasyprint is an open source project and produces decent PDFs, but is
    # slow and doesn't always obey the HTML and CSS formatting exactly,
    # e.g., sometimes text is flowed around chapter numbers as it should be
    # and sometimes it isn't (when CSS dictates it). It is a bug in
    # weasyprint. Princexml is a top of class closed source solution.
    # Princexml is the fastest HTML to PDF converter known and its fidelity
    # to CSS is best in class. We are able to use Princexml under a
    # non-commercial license since it leaves a Princexml logo on the first
    # page of the generated PDF. The logo isn't too obtrusive. One may
    # choose to use Princexml rather than Weasyprint by setting use_prince
    # to True.
    use_prince: Annotated[bool, AffectsOutput()] = False
    # Some languages, e.g., Khmer, don't layout well in 2 column
    use_two_column_layout_for_tn_notes: Annotated[bool, AffectsOutput()] = False
    # Some languages, e.g., Khmer, don't layout well in 2 column
    use_two_column_layout_for_tq_notes: Annotated[bool, AffectsOutput()] = False
    # Indicate whether to show visual separator between sections, e.g., hr element
    use_section_visual_separator: Annotated[bool, AffectsOutput()] = False
    show_tn_book_intro: Annotated[bool, AffectsOutput()] = True
    show_bc_book_intro: Annotated[bool, AffectsOutput()] = True
    show_tn_chapter_intro: Annotated[bool, AffectsOutput()] = True
    show_bc_chapter_commentary: Annotated[bool, AffectsOutput()] = True
    # Indicate where the document request originated from. We default to
    # TEST so that tests don't have to specify and every other client, e.g.,
    # UI, should specify in order for
    # document_generator.select_assembly_layout_kind to produce
    # expected results.
    document_request_source: Annotated[DocumentRequestSourceEnum, DeliveryOnly()] = (
        DocumentRequestSourceEnum.TEST
    )

    @model_validator(mode="after")
    def ensure_valid_document_request(self) -> "DocumentRequest":
        """
        See ValueError messages below for the rules we are enforcing.
        """
        usfm_resource_types = settings.USFM_RESOURCE_TYPES
        non_usfm_resource_types = [
            EN_TN_CONDENSED_RESOURCE_TYPE,
            TN_RESOURCE_TYPE,
            TQ_RESOURCE_TYPE,
            TW_RESOURCE_TYPE,
            BC_RESOURCE_TYPE,
        ]
        all_resource_types = [*usfm_resource_types, *non_usfm_resource_types]
        if not self.resource_requests:
            raise ValueError("DocumentRequest has no resource requests.")
        for resource_request in self.resource_requests:
            # Make sure resource_type for every ResourceRequest instance
            # is a valid value
            if resource_request.resource_type not in all_resource_types:
                raise ValueError(
                    f"{resource_request.resource_type} is not a valid resource type"
                )
            # Make sure book_code is a valid value
            if resource_request.book_code not in BOOK_NAMES.keys():
                raise ValueError(
                    f"{resource_request.book_code} is not a valid book code"
                )
        # Partition USFM resource requests by language
        language_groups: dict[str, list[ResourceRequest]] = {}
        for resource in filter(
            lambda r: r.resource_type in usfm_resource_types, self.resource_requests
        ):
            lang_code = resource.lang_code
            if lang_code not in language_groups:
                language_groups[lang_code] = []
            language_groups[lang_code].append(resource)
        # Get a list of the sorted set of books for each language for later
        # comparison.
        sorted_book_set_for_each_language = [
            sorted({item.book_code for item in value})
            for key, value in language_groups.items()
        ]
        # Next two assignments are later used to do a not all equal check.
        # Check if all sets have equal lengths
        are_lengths_equal = all(
            len(set_) == len(sorted_book_set_for_each_language[0])
            for set_ in sorted_book_set_for_each_language
        )
        # Check if all sets have equal elements
        are_sets_equal = all(
            set_ == sorted_book_set_for_each_language[0]
            for set_ in sorted_book_set_for_each_language
        )
        # Get the unique number of languages
        number_of_usfm_languages = len(
            # set(
            [
                resource_request.lang_code
                for resource_request in self.resource_requests
                if resource_request.resource_type in usfm_resource_types
            ]
            # )
        )
        if (
            self.assembly_strategy_kind != AssemblyStrategyEnum.INTERLEAVE_BY_CHAPTER
            and self.assembly_layout_kind
            == AssemblyLayoutEnum.TWO_COLUMN_SCRIPTURE_LEFT_SCRIPTURE_RIGHT
        ):
            raise ValueError(
                "Two column scripture left, scripture right layout is only compatible with book language order assembly strategy."
            )
        elif (
            self.assembly_strategy_kind == AssemblyStrategyEnum.INTERLEAVE_BY_CHAPTER
            and self.assembly_layout_kind
            == AssemblyLayoutEnum.TWO_COLUMN_SCRIPTURE_LEFT_SCRIPTURE_RIGHT
            # Because book content for different languages will be side by side for
            # the scripture left scripture right layout, we make sure there are a non-zero
            # even number of languages so that we can display them left and right in
            # pairs.
            and not is_even(number_of_usfm_languages)
        ):
            raise ValueError(
                "Two column scripture left, scripture right layout requires a non-zero even number of languages. For an uneven number of languages you'll want to use the one column layout kind."
            )
        elif (
            self.assembly_strategy_kind == AssemblyStrategyEnum.INTERLEAVE_BY_CHAPTER
            and self.assembly_layout_kind
            == AssemblyLayoutEnum.TWO_COLUMN_SCRIPTURE_LEFT_SCRIPTURE_RIGHT
            # Because book content for different languages will be side by side for
            # the scripture left scripture right layout, we make sure there are a non-zero
            # even number of languages so that we can display them left and right in
            # pairs.
            # and number_of_usfm_languages > 1
            and is_even(number_of_usfm_languages)
            # Each language must have the same set of books in order to
            # use the scripture left scripture right layout strategy. As an example,
            # you wouldn't want to allow the sl-sr layout if the document request
            # asked for Swahili ulb for Lamentations and Spanish ulb for Nahum -
            # the set of books in each language are not the same and so do not make
            # sense to be displayed side by side.
            # and not all_equal(sorted_book_set_for_each_language)
            and not (are_lengths_equal and are_sets_equal)
        ):
            raise ValueError(
                "Two column scripture left, scripture right layout requires the same books for each language chosen since they are displayed side by side. If you want a different set of books for each language you'll instead need to use the one column layout."
            )
        return self


@final
class LangDirEnum(str, Enum):
    """
    This class/enum enumerates the possible language display
    directions: LTR or RTL.
    """

    LTR = "ltr"
    RTL = "rtl"


@final
class ResourceLookupDto(NamedTuple):
    """
    'Data transfer object' that we use to send resource lookup related
    info around in the system.
    """

    lang_code: str
    lang_name: str
    localized_lang_name: str
    resource_type: str
    resource_type_name: str
    book_code: str
    lang_direction: LangDirEnum
    url: Optional[HttpUrl]


@final
@dataclass
class TNChapter:
    intro_html: str
    verses: dict[VerseRef, str]


@final
@dataclass
class TNBook:
    lang_code: str
    lang_name: str
    book_code: str
    resource_type_name: str
    book_intro: str
    chapters: dict[ChapterNum, TNChapter]
    lang_direction: LangDirEnum


@final
@dataclass
class TNCChapter:
    intro_html: str
    verses: dict[VerseRef, str]


@final
@dataclass
class TNCBook:
    lang_code: str
    lang_name: str
    book_code: str
    resource_type_name: str
    book_intro: str
    chapters: dict[ChapterNum, TNCChapter]
    lang_direction: LangDirEnum


@final
@dataclass
class TQChapter:
    verses: dict[VerseRef, str]


@final
@dataclass
class TQBook:
    lang_code: str
    lang_name: str
    book_code: str
    resource_type_name: str
    chapters: dict[ChapterNum, TQChapter]
    lang_direction: LangDirEnum


@final
class TWUse(NamedTuple):
    """
    A class to reify a reference to a translation word occurring
    in a USFM verse.
    """

    lang_code: str
    book_id: str
    book_name: str
    chapter_num: ChapterNum
    verse_num: VerseRef
    localized_word: str


# `content' gets mutated after instantiation therefore we can't subclass NamedTuple
@final
class TWNameContentPair:
    """
    A class to hold the localized translation word and its associated
    HTML content.
    """

    def __init__(self, localized_word: str, path: str, content: str):
        self.localized_word = localized_word
        self.path = path
        self.content = content


@final
class TWBook(NamedTuple):
    lang_code: str
    lang_name: str
    resource_type_name: str
    lang_direction: LangDirEnum
    name_content_pairs: list[TWNameContentPair] = []
    uses: dict[str, list[TWUse]] = {}


@final
class BCChapter(NamedTuple):
    commentary: str


@final
class BCBook(NamedTuple):
    """
    A class to hold a mapping of chapter numbers to translation questions
    HTML content.
    """

    book_intro: str
    lang_code: str
    lang_name: str
    book_code: str
    resource_type_name: str
    chapters: dict[ChapterNum, BCChapter]
    lang_direction: LangDirEnum = LangDirEnum.LTR


@final
class USFMChapter(BaseModel):
    """
    A class to hold the USFM converted to HTML content for a chapter
    in total (including things like 'chunk breaks' and other verse
    formatting HTML elements), content, and chapter verses, i.e., missing
    the 'chunk breaks' and other inter-verse HTML formatting elements.
    The purpose of 'content' is so that you can display a whole
    chapter at a time when desired. The purpose of 'verses' is so that you
    can display verses in a particular chapter one at a time or a range of
    them at a time.
    """

    content: str
    verses: Optional[dict[VerseRef, str]]


@final
class USFMBook(BaseModel):
    """A class to hold a book's USFMChapter instances."""

    lang_code: str
    lang_name: str
    localized_lang_name: str
    book_code: str
    national_book_name: str
    resource_type_name: str
    chapters: dict[ChapterNum, USFMChapter]
    lang_direction: LangDirEnum


@final
class Attachment(NamedTuple):
    """
    An email attachment.
    """

    filepath: str
    mime_type: tuple[str, str]


# Models for manifest files and data:


class YamlManifestBook(TypedDict):
    title: str
    identifier: str


class JsonManifestBook(TypedDict):
    name: str
    id: str


class Data(TypedDict):
    projects: list[YamlManifestBook]


class JsonManifestData(TypedDict):
    project: JsonManifestBook


# Model the source data returned in fetch_source_data from the data API:


class Language(BaseModel):
    english_name: str
    ietf_code: str
    national_name: str
    direction: LangDirEnum


class Content(BaseModel):
    resource_type: str
    language: Language


class RepoEntry(BaseModel):
    repo_url: HttpUrl
    content: Content


class SourceData(BaseModel):
    git_repo: list[RepoEntry]


# Model parts of the generated document:


class DocumentPart(BaseModel):
    content: str
    is_rtl: bool = False
    contained_in_two_column_section: bool = False
    add_page_break: bool = False
    use_section_visual_separator: bool = False
