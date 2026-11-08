from typing import Mapping

from bs4 import BeautifulSoup, NavigableString, Tag
from doc.config import get_settings
from doc.domain.bible_books import BOOK_CHAPTER_VERSES
from doc.domain.model import USFMBook
from doc.domain.parsing import lookup_verse_text
from passages.reviewers_guide.model import BibleReference

logger = get_settings().logger(__name__)


def _unwrap_verse_text(raw_text: str) -> str:
    """
    If the stored verse text is already wrapped in <span class="verse">,
    return its inner content; otherwise return the text as-is.
    """
    soup = BeautifulSoup(raw_text, "html.parser")
    outer = soup.find("span", class_="verse")
    if isinstance(outer, Tag):
        return outer.decode_contents().strip()
    return raw_text.strip()


def _make_verse_span(verse_number: int | str, raw_text: str) -> str:
    """Wrap cleaned verse text in a canonical <span class="verse"> with a versemarker."""
    text = _unwrap_verse_text(raw_text)
    if not text:
        return ""
    return (
        f'<span class="verse">'
        f'<sup class="versemarker">{verse_number}</sup>'
        f"{text}"
        f"</span>"
    )


def _verses_in_range(
    usfm_book: USFMBook,
    chapter: int,
    lower: int,
    upper: int,
) -> list[str]:
    """Return verse spans for every verse in [lower, upper] within chapter."""
    spans = []
    for idx in range(lower, upper + 1):
        raw = lookup_verse_text(usfm_book, chapter, str(idx))
        if raw:
            span = _make_verse_span(idx, raw)
            if span:
                spans.append(span)
                logger.debug("verse_text: %s", span)
    return spans


def _verses_for_ref(
    usfm_book: USFMBook,
    chapter: int,
    verse_ref: str,
) -> list[str]:
    """
    Parse a verse reference string (single verse, range, or comma list)
    and return the corresponding verse spans.
    Supported formats:
      "5"        -> verse 5
      "3-7"      -> verses 3 through 7
      "1,3-5,8"  -> verse 1, verses 3-5, verse 8
    """
    spans = []
    verse_ref = verse_ref.strip()
    for part in verse_ref.split(","):
        part = part.strip()
        if "-" in part:
            lo_str, hi_str = part.split("-", 1)
            spans.extend(_verses_in_range(usfm_book, chapter, int(lo_str), int(hi_str)))
        else:
            raw = lookup_verse_text(usfm_book, chapter, part)
            if raw:
                span = _make_verse_span(part, raw)
                if span:
                    spans.append(span)
                    logger.debug("verse_text: %s", span)
    return spans


def verse_text_html(
    bible_reference: BibleReference,
    usfm_book: USFMBook,
    book_chapter_verses: Mapping[str, Mapping[str, str]] = BOOK_CHAPTER_VERSES,
) -> str:
    spans: list[str] = []
    if (
        bible_reference.end_chapter
        and bible_reference.end_chapter > 0
        and bible_reference.end_chapter_verse_ref
    ):
        # Cross-chapter range: gather tail of start chapter + head of end chapter
        start_chapter_last_verse = int(
            book_chapter_verses[bible_reference.book_code][
                str(bible_reference.start_chapter)
            ]
        )
        spans.extend(
            _verses_in_range(
                usfm_book,
                bible_reference.start_chapter,
                int(bible_reference.start_chapter_verse_ref),
                start_chapter_last_verse,
            )
        )
        spans.extend(
            _verses_in_range(
                usfm_book,
                bible_reference.end_chapter,
                1,
                int(bible_reference.end_chapter_verse_ref),
            )
        )
    else:
        spans.extend(
            _verses_for_ref(
                usfm_book,
                bible_reference.start_chapter,
                bible_reference.start_chapter_verse_ref,
            )
        )
    return "".join(spans)


def clean_chapter_html(chapter: str) -> str:
    verse_dict: dict[str, str] = {}
    soup = BeautifulSoup(chapter, "html.parser")
    for verse_span in soup.find_all("span", class_="verse"):
        # Remove footnote callers
        for caller in verse_span.find_all("sup", class_="caller"):
            caller.decompose()
        # Fix spacing issue for poetry divs
        for poetry_div in verse_span.find_all(
            "div", class_=lambda c: c and c.startswith("poetry-")
        ):
            poetry_div.insert_before(NavigableString(" "))
    return str(soup)
