import re

import pytest
from doc.domain.parsing import (
    ensure_chapter_label,
    ensure_chapter_marker,
    maybe_localized_book_name,
    split_chapter_into_verses_with_formatting,
)
from doc.domain import model, resource_lookup
from doc.domain.model import USFMChapter


def test_ensure_chapter_marker_unchanged_if_exists() -> None:
    chapter_num = 5

    # Case 1: Chapter marker already exists (should remain unchanged)
    existing_chapter = "\\c 5\nSome text."
    assert ensure_chapter_marker(existing_chapter, chapter_num) == existing_chapter

    # Case 6: Text already has a different chapter number (should remain unchanged)
    existing_different_chapter = "\\c 10\nText continues."
    assert (
        ensure_chapter_marker(existing_different_chapter, chapter_num)
        == existing_different_chapter
    )


def test_ensure_chapter_marker_inserted_at_beginning() -> None:
    chapter_num = 5
    # Case 2: No chapter marker, insert at beginning
    no_chapter_marker = "Some text without a chapter marker."
    expected_output = f"\\c {chapter_num}\nSome text without a chapter marker."
    actual_output = ensure_chapter_marker(no_chapter_marker, chapter_num)
    print("actual: " + repr(actual_output))  # Print raw string representation
    print("expected: " + repr(expected_output))
    assert actual_output == expected_output

    # Case 4: Text with multiple lines, no \c, insert at start
    multiline_text = "Line 1\nLine 2\nLine 3"
    expected_output = f"\\c {chapter_num}\nLine 1\nLine 2\nLine 3"
    actual_output = ensure_chapter_marker(multiline_text, chapter_num)
    print("actual: " + repr(actual_output))  # Print raw string representation
    print("expected: " + repr(expected_output))
    assert actual_output == expected_output


def test_ensure_chapter_marker_inserted_at_before_chapter_label() -> None:
    chapter_num = 5
    # Case 3: Chapter marker missing, but \cl exists (insert before \cl)
    text_with_cl = "\\cl Chapter Title\nSome text."
    expected_output = f"\n\\c {chapter_num}\n\\cl Chapter Title\nSome text."
    actual_output = ensure_chapter_marker(text_with_cl, chapter_num)
    print("actual: " + repr(actual_output))  # Print raw string representation
    print("expected: " + repr(expected_output))
    assert actual_output == expected_output


def test_ensure_chapter_marker_inserted() -> None:
    chapter_num = 5

    # Case 5: Text with \cl but no \c, should insert before \cl
    complex_text = "\\id mat\n\\cl Gospel of Matthew\nText starts here."
    expected_output = "\\id mat\n\n\\c 5\n\\cl Gospel of Matthew\nText starts here."
    actual_output = ensure_chapter_marker(complex_text, chapter_num)
    print("actual: " + repr(actual_output))  # Print raw string representation
    print("expected: " + repr(expected_output))
    assert actual_output == expected_output


def test_adds_missing_chapter_label() -> None:
    input_text = "\n\\c 1\n\\v 1 In the beginning..."
    expected_output = "\n\n\\c 1\n\\cl Chapter 1\n\n\\v 1 In the beginning..."
    actual_output = ensure_chapter_label(input_text, 1)
    print("actual: " + repr(actual_output))  # Print raw string representation
    print("expected: " + repr(expected_output))
    assert actual_output == expected_output


def test_keeps_existing_chapter_label() -> None:
    input_text = "\n\\c 1\n\\cl Chapter\n\\v 1 In the beginning..."
    expected_output = "\n\\c 1\n\\cl Chapter 1\n\\v 1 In the beginning..."
    assert ensure_chapter_label(input_text, 1) == expected_output


def test_no_chapter_marker() -> None:
    input_text = "\n\\v 1 In the beginning..."
    assert ensure_chapter_label(input_text, 1) == input_text


def test_fr_f10_book_name_lookup_prefs() -> None:
    usfm_metadata = r"""\id JUD
\h ÉPÎTRE DE SAINT JUDE
\toc1 ÉPÎTRE DE SAINT JUDE
\toc2 Épître de Jude
\toc3 Jude
\mt1 ÉPÎTRE DE SAINT JUDE

\s5
"""
    expected = "Épître de Jude"
    localized_book_name = maybe_localized_book_name(usfm_metadata, "fr", "f10")
    assert localized_book_name != "Épître de saint jude"
    assert localized_book_name == expected


# Sample content taken from fr f10 Matthew 1, which uses word-entry tags.
FRENCH_WORD_ENTRY_HTML = """
<span class="verse">
<sup class="versemarker">1</sup>
<span class="word-entry"> Généalogie </span>
<span class="word-entry">  </span>
 de
<span class="word-entry"> Jésus </span>
-
<span class="word-entry"> Christ </span>
,
<span class="word-entry"> fils </span>
 de
<span class="word-entry"> David </span>
,
<span class="word-entry"> fils </span>
 d'
<span class="word-entry"> Abraham </span>
.

</span>
<span class="verse">
<sup class="versemarker">2</sup>
<span class="word-entry"> Abraham </span>

<span class="word-entry"> engendra </span>

<span class="word-entry"> Isaac </span>
;
</span>
"""

# Sample content in the shape produced for USFM without word-entry tags: a
# footnote caller sup and a trailing sectionhead div follow the verse text.
GALATIANS_HTML = """
<span class="verse">
<sup class="versemarker"> 19 </sup>
For through the law I died to the law, so that I might live for God.
<sup id="footnote-caller-1" class="caller"><a href="#footnote-target-1">1</a></sup>
<div class="sectionhead-5"></div>
</span>
<span class="verse">
<sup class="versemarker">20</sup>
I have been crucified with Christ and I no longer live.
<sup id="footnote-caller-2" class="caller"><a href="#footnote-target-2">2</a></sup>
<div class="sectionhead-5"></div>
</span>
"""


def test_split_chapter_into_verses_with_formatting_keys() -> None:
    chapter = USFMChapter(content=GALATIANS_HTML, verses=None)
    verses = split_chapter_into_verses_with_formatting(chapter)
    assert list(verses.keys()) == ["19", "20"]


def test_split_chapter_into_verses_with_formatting_strips_verse_number_whitespace() -> (
    None
):
    # The versemarker sup for verse 19 is written as "<sup ...> 19 </sup>".
    chapter = USFMChapter(content=GALATIANS_HTML, verses=None)
    verses = split_chapter_into_verses_with_formatting(chapter)
    assert "19" in verses
    assert " 19 " not in verses


def test_split_chapter_into_verses_with_formatting_removes_versemarker_only() -> None:
    chapter = USFMChapter(content=GALATIANS_HTML, verses=None)
    verses = split_chapter_into_verses_with_formatting(chapter)
    for verse in verses.values():
        assert "versemarker" not in verse
    # Other markup inside the verse span survives.
    assert verses["19"] == (
        '<span class="verse"> For through the law I died to the law, '
        "so that I might live for God.\n"
        '<sup class="caller" id="footnote-caller-1">'
        '<a href="#footnote-target-1">1</a></sup>\n'
        '<div class="sectionhead-5"></div>\n</span>'
    )


def test_split_chapter_into_verses_with_formatting_unwraps_word_entries() -> None:
    chapter = USFMChapter(content=FRENCH_WORD_ENTRY_HTML, verses=None)
    verses = split_chapter_into_verses_with_formatting(chapter)
    assert list(verses.keys()) == ["1", "2"]
    for verse in verses.values():
        assert "word-entry" not in verse
    # The wrapped text survives in place, with whitespace collapsed and
    # whitespace before punctuation removed.
    assert verses["1"] == (
        '<span class="verse"> Généalogie de Jésus-Christ, fils de David, '
        "fils d'Abraham. </span>"
    )
    assert verses["2"] == '<span class="verse"> Abraham engendra Isaac;\n</span>'


def test_split_chapter_into_verses_with_formatting_collapses_hyphen_spacing() -> None:
    """Spacing around a hyphen is intentionally collapsed (see clean_content_html)."""
    chapter = USFMChapter(content=FRENCH_WORD_ENTRY_HTML, verses=None)
    verses = split_chapter_into_verses_with_formatting(chapter)
    assert "Jésus-Christ" in verses["1"]
    assert "Jésus - Christ" not in verses["1"]


def test_split_chapter_into_verses_with_formatting_skips_verses_without_versemarker() -> (
    None
):
    html_content = """
<span class="verse">
No versemarker sup at all here.
</span>
<span class="verse">
<sup class="versemarker"></sup>
An empty versemarker sup here.
</span>
<span class="verse">
<sup class="versemarker">3</sup>
A well formed verse.
</span>
"""
    chapter = USFMChapter(content=html_content, verses=None)
    verses = split_chapter_into_verses_with_formatting(chapter)
    assert list(verses.keys()) == ["3"]
    assert verses["3"] == '<span class="verse"> A well formed verse.\n</span>'


def test_split_chapter_into_verses_with_formatting_without_verse_spans() -> None:
    chapter = USFMChapter(
        content="<p>Chapter content with no verse spans.</p>", verses=None
    )
    assert split_chapter_into_verses_with_formatting(chapter) == {}


def test_split_chapter_into_verses_with_formatting_empty_content() -> None:
    chapter = USFMChapter(content="", verses=None)
    assert split_chapter_into_verses_with_formatting(chapter) == {}


if __name__ == "__main__":
    pytest.main()
