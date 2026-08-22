from re import sub
from typing import Mapping

from bs4 import BeautifulSoup, NavigableString
from doc.config import settings
from doc.domain.bible_books import BOOK_CHAPTER_VERSES
from doc.domain.model import USFMBook, USFMChapter
from doc.domain.parsing import lookup_verse_text
from doc.reviewers_guide.model import BibleReference

logger = settings.logger(__name__)


def verse_text_html(
    bible_reference: BibleReference,
    usfm_book: USFMBook,
    book_chapter_verses: Mapping[str, Mapping[str, str]] = BOOK_CHAPTER_VERSES,
) -> str:
    verse_text = []
    if (
        bible_reference.end_chapter
        and bible_reference.end_chapter > 0
        and bible_reference.end_chapter_verse_ref
    ):  # chapter boundary traversal
        start_chapter_lower_verse = int(bible_reference.start_chapter_verse_ref)
        start_chapter_upper_verse = int(
            book_chapter_verses[bible_reference.book_code][
                str(bible_reference.start_chapter)
            ]
        )
        for idx in range(start_chapter_lower_verse, start_chapter_upper_verse + 1):
            start_chapter_verse_text = lookup_verse_text(
                usfm_book,
                bible_reference.start_chapter,
                str(idx),
            )
            if start_chapter_verse_text:
                verse_text.append(
                    f'<span class="verse"><sup class="versemarker">{str(idx)}</sup>{start_chapter_verse_text}</span>'
                )
        end_chapter_lower_verse = 1
        end_chapter_upper_verse = int(bible_reference.end_chapter_verse_ref)
        for idx in range(end_chapter_lower_verse, end_chapter_upper_verse + 1):
            end_chapter_verse_text = lookup_verse_text(
                usfm_book,
                bible_reference.end_chapter,
                str(idx),
            )
            if end_chapter_verse_text:
                verse_text.append(
                    f'<span class="verse"><sup class="versemarker">{str(idx)}</sup>{end_chapter_verse_text}</span>'
                )
    else:
        if "," in bible_reference.start_chapter_verse_ref:
            verse_range_components = bible_reference.start_chapter_verse_ref.split(",")
            for verse_ in verse_range_components:
                if "-" in verse_:
                    verse__range_components = verse_.split("-")
                    lower_verse_ = int(verse__range_components[0])
                    upper_verse_ = int(verse__range_components[1])
                    for idx in range(lower_verse_, upper_verse_ + 1):
                        verse_text__ = lookup_verse_text(
                            usfm_book,
                            bible_reference.start_chapter,
                            str(idx),
                        )
                        if verse_text__:
                            verse_text.append(
                                f'<span class="verse"><sup class="versemarker">{str(idx)}</sup>{verse_text__}</span>'
                            )
                else:
                    verse_text__ = lookup_verse_text(
                        usfm_book,
                        bible_reference.start_chapter,
                        verse_,
                    )
                    if verse_text__:
                        verse_text.append(
                            f'<span class="verse"><sup class="versemarker">{verse_}</sup>{verse_text__}</span>'
                        )
        elif "-" in bible_reference.start_chapter_verse_ref:
            verse_range_components = bible_reference.start_chapter_verse_ref.split("-")
            lower_verse = int(verse_range_components[0])
            upper_verse = int(verse_range_components[1])
            for idx in range(lower_verse, upper_verse + 1):
                verse_text_ = lookup_verse_text(
                    usfm_book,
                    bible_reference.start_chapter,
                    str(idx),
                )
                if verse_text_:
                    verse_text.append(
                        f'<span class="verse"><sup class="versemarker">{str(idx)}</sup>{verse_text_}</span>'
                    )
        else:
            verse_text___ = lookup_verse_text(
                usfm_book,
                bible_reference.start_chapter,
                bible_reference.start_chapter_verse_ref.strip(),
            )
            if verse_text___:
                verse_text.append(
                    f'<span class="verse"><sup class="versemarker">{bible_reference.start_chapter_verse_ref.strip()}</sup>{verse_text___}</span>'
                )
    return "".join(verse_text)


def split_chapter_into_verses(chapter: USFMChapter) -> dict[str, str]:
    # Sample HTML content with multiple verse elements
    # html_content = '''
    # <span class="verse">
    # <sup class="versemarker">19</sup>
    # For through the law I died to the law, so that I might live for God. I have been crucified with Christ.
    # <sup id="footnote-caller-1" class="caller"><a href="#footnote-target-1">1</a></sup>
    # <div class="sectionhead-5"></div>
    # </span>
    # <span class="verse">
    # <sup class="versemarker">20</sup>
    # I have been crucified with Christ and I no longer live, but Christ lives in me. The life I now live in the body, I live by faith in the Son of God, who loved me and gave himself for me.
    # <sup id="footnote-caller-2" class="caller"><a href="#footnote-target-2">2</a></sup>
    # <div class="sectionhead-5"></div>
    # </span>
    # '''
    verse_dict: dict[str, str] = {}
    soup = BeautifulSoup(chapter.content, "html.parser")
    for verse_span in soup.find_all("span", class_="verse"):
        versemarker = verse_span.find("sup", class_="versemarker")
        if not versemarker or not versemarker.get_text(strip=True):
            continue
        verse_number = versemarker.get_text(strip=True)
        # Remove verse marker
        versemarker.decompose()
        # Remove footnote callers
        for caller in verse_span.find_all("sup", class_="caller"):
            caller.decompose()
        # Fix spacing issue for poetry divs
        for poetry_div in verse_span.find_all(
            "div", class_=lambda c: c and c.startswith("poetry-")
        ):
            poetry_div.insert_before(NavigableString(" "))
        # Handle fr f10 word-entry tags
        for we in verse_span.find_all("span", class_="word-entry"):
            we.unwrap()
        # Get inner HTML of the verse span
        verse_text = "".join(str(child) for child in verse_span.contents).strip()
        verse_text = clean_verse_html(verse_text)
        verse_dict[verse_number] = verse_text
    return verse_dict


def clean_verse_html(
    raw_verse: str,
    empty_paragraph: str = "<p></p>",
    sectionhead5_element: str = '<div class="sectionhead-5"></div>',
) -> str:
    cleaned_html = raw_verse
    cleaned_html = cleaned_html.replace(empty_paragraph, " ").replace(
        sectionhead5_element, " "
    )
    cleaned_html = sub(r"\s+([,;:.!?])", r"\1", cleaned_html)
    cleaned_html = sub(r"\s+'", "'", cleaned_html)
    cleaned_html = sub(r"'\s+", "'", cleaned_html)
    cleaned_html = sub(r"\s*-\s*", "-", cleaned_html)
    cleaned_html = sub(r"\s{2,}", " ", cleaned_html).strip()
    return cleaned_html
