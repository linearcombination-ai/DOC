"""
Rendering constants and domain tables used when generating documents.

These used to be fields of config.Settings, but they are not
deployment configuration: nothing sets them from the environment and
changing them changes generated output. Hence they are plain immutable
module constants.
"""

from types import MappingProxyType
from typing import Final, Mapping, Sequence

PREFERRED_RESOURCES: Final[Mapping[str, str]] = MappingProxyType(
    {"fr": "f10", "id": "tbi"}
)


# This is only used to see if a lang_code is in the collection
# otherwise it is a heart language. Eventually the graphql data api may
# provide gateway/heart boolean value.
GATEWAY_LANGUAGES: Final[frozenset[str]] = frozenset(
    [
        "abs",
        "aju",
        "am",
        "apd",
        "ar",
        "ar-x-dcv",
        "ary",
        "arz",
        "as",
        "ase",
        "bem",
        "bg",
        "bgw",
        "bi",
        "bn",
        "ceb",
        "cmn",
        "cmn-x-omc",
        "csl",
        "dz",
        "en",
        "es",
        "es-419",
        "fa",
        "fil",
        "fr",
        "grt",
        "gu",
        "gug",
        "ha",
        "hbs",
        "hca",
        "he",
        "hi",
        "hne",
        "hu",
        "id",
        "id-x-dcv",
        "idb",
        "ilo",
        "ins",
        "ja",
        "jv",
        "kas",
        "km",
        "kn",
        "lbj",
        "ln",
        "lo",
        "mai",
        "mg",
        "ml",
        "mn",
        "mni",
        "mnk",
        "mr",
        "ms",
        "my",
        "ne",
        "nl",
        "npi",
        "or",
        "pa",
        "pbt",
        "pes",
        "pis",
        "plt",
        "pmy",
        "pnb",
        "prs",
        "ps",
        "psr",
        "pt",
        "pt-br",
        "raj",
        "rsl",
        "ru",
        "rwr",
        "sn",
        "sw",
        "swc",
        "swh",
        "ta",
        "te",
        "th",
        "ti",
        "tl",
        "tn",
        "tpi",
        "tr",
        "tsg",
        "ug",
        "ur",
        "vi",
        "zh",
        "zlm",
    ]
)


USFM_RESOURCE_TYPES: Final[Sequence[str]] = (
    "avd",
    "ayt",
    "bpb",
    "blv",
    "cuv",
    "f10",
    "nav",
    "reg",
    "tbi",
    # "udb",  # Content team doesn't want udb used for any language, e.g., en, mr, tl, gu
    "ugnt",
    "uhb",
    "ulb",
    "usfm",
)


# This can be expanded to include any additional types (if
# there are any) that we want to be available to users. These are all
# that I found of relevance in the data API.
RESOURCE_TYPE_CODES_AND_NAMES: Final[Mapping[str, str]] = MappingProxyType(
    {
        "ayt": "Alkitab Yang Terbuka (Easy-to-Read Indonesian Bible)",
        "bc": "Bible Commentary",
        "blv": "Portuguese Bíblia Livre",
        "bpb": "Bulgarian Protestant Bible",
        "cuv": "新标点和合本",
        "f10": "French Louis Segond 1910 Bible",
        "nav": "New Arabic Version (Ketab El Hayat)",
        "reg": "Regular",
        "rg": "NT Survey Reviewers' Guide",
        "tbi": "Terjemahan Baru (Indonesian New Translation Bible)",
        "tn": "Translation Notes",
        "tn-condensed": "Condensed Translation Notes",
        "tq": "Translation Questions",
        "tw": "Translation Words",
        # "udb": "Unlocked Dynamic Bible",  # Content team doesn't want udb used for any language, e.g., en, mr, tl, gu
        "ugnt": "unfoldingWord® Greek New Testament",
        "uhb": "unfoldingWord® Hebrew Bible",
        "ulb": "Unlocked Literal Bible",
    }
)


TRANSLATION_WORD_VERSE_SECTION_HEADER_STR: Final[str] = "<h4>Uses:</h4>"
TRANSLATION_WORD_VERSE_REF_ITEM_FMT_STR: Final[str] = (
    '<li><a href="#{}-{}-ch-{}-v-{}">{} {}:{}</a></li>'
)
UNORDERED_LIST_BEGIN_STR: Final[str] = "<ul>"
UNORDERED_LIST_END_STR: Final[str] = "</ul>"
VERSE_SPAN_FMT_STR: Final[str] = '<span class="verse">{}</span>'
BOOK_NAME_FMT_STR: Final[str] = "<h2 class='book-name'>{}</h2>"
RESOURCE_TYPE_NAME_FMT_STR: Final[str] = "<h1 class='book-name'>{}</h1>"
HR: Final[str] = "<hr/>"
BIEL_TW_RESOURCE_URL_FMT_STR: Final[str] = (
    "<span><a href='https://bibleineverylanguage.org/resources/languages/{}?resource-type=tw'>{}</a></span>"
)
TW_RESOURCE_URL_FMT_STR: Final[str] = "<span><a href='#{}-{}'>{}</a></span>"
