from doc.domain.model import USFMChapter
from passages.domain.parser import clean_verse_html, split_chapter_into_verses

# Same fixtures used by tests/unit/test_parsing.py to specify the sibling
# DOC-path function's behavior for this exact content shape.
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

# Mimics a Psalm 119-style acrostic heading: an empty <p></p> (produced by
# the USFM converter closing a bare \p before a \qa heading it can't nest)
# sitting between poetry lines, with the acrostic heading itself carried in
# a separate <span class="acrostic-heading">.
ACROSTIC_ADJACENT_HTML = """
<span class="verse">
<sup class="versemarker">9</sup>
<div class="poetry-1">How can a young man keep his way pure?</div>
<div class="poetry-2">By guarding it according to your word.</div>
<p></p>
<span class="acrostic-heading">Bet</span>
<div class="poetry-1">With my whole heart I seek you;</div>
<div class="poetry-2">let me not wander from your commandments!</div>
</span>
"""


def test_split_chapter_into_verses_cleans_galatians_prose() -> None:
    chapter = USFMChapter(content=GALATIANS_HTML, verses=None)
    verses = split_chapter_into_verses(chapter)
    assert verses["19"] == (
        "For through the law I died to the law, so that I might live for God."
    )
    assert verses["20"] == (
        "I have been crucified with Christ and I no longer live."
    )
    assert "sectionhead-5" not in verses["19"]
    assert "sectionhead-5" not in verses["20"]


def test_split_chapter_into_verses_cleans_french_word_entry_prose() -> None:
    chapter = USFMChapter(content=FRENCH_WORD_ENTRY_HTML, verses=None)
    verses = split_chapter_into_verses(chapter)
    assert verses["1"] == "Généalogie de Jésus-Christ, fils de David, fils d'Abraham."
    assert verses["2"] == "Abraham engendra Isaac;"


def test_clean_verse_html_sectionhead5_abutting_text_keeps_words_separated() -> None:
    # Regression test: a sectionhead-5 div directly touching text with no
    # surrounding whitespace must not merge the two adjacent words.
    raw = 'the righteous<div class="sectionhead-5"></div>shall live by faith'
    assert clean_verse_html(raw) == "the righteous shall live by faith"


def test_clean_verse_html_empty_paragraph_abutting_text_keeps_words_separated() -> None:
    raw = "the righteous<p></p>shall live by faith"
    assert clean_verse_html(raw) == "the righteous shall live by faith"


def test_clean_verse_html_sectionhead5_with_surrounding_whitespace_no_double_space() -> (
    None
):
    raw = 'righteous <div class="sectionhead-5"></div> shall live'
    result = clean_verse_html(raw)
    assert result == "righteous shall live"
    assert "  " not in result


def test_clean_verse_html_empty_paragraph_with_surrounding_whitespace_no_double_space() -> (
    None
):
    raw = "righteous <p></p> shall live"
    result = clean_verse_html(raw)
    assert result == "righteous shall live"
    assert "  " not in result


def test_clean_verse_html_leaves_unrelated_nonempty_div_untouched() -> None:
    raw = (
        "The word became flesh.\n"
        '<div class="footnote-content">See also John 1:14.</div>'
    )
    assert clean_verse_html(raw) == raw


def test_split_chapter_into_verses_preserves_poetry_and_acrostic_heading() -> None:
    chapter = USFMChapter(content=ACROSTIC_ADJACENT_HTML, verses=None)
    verses = split_chapter_into_verses(chapter)
    verse = verses["9"]
    # Poetry line structure survives intact.
    assert '<div class="poetry-1">How can a young man keep his way pure?</div>' in verse
    assert (
        '<div class="poetry-2">By guarding it according to your word.</div>' in verse
    )
    assert '<div class="poetry-1">With my whole heart I seek you;</div>' in verse
    assert (
        '<div class="poetry-2">let me not wander from your commandments!</div>'
        in verse
    )
    # The acrostic heading content survives, and isn't merged into
    # neighboring poetry text.
    assert '<span class="acrostic-heading">Bet</span>' in verse
    assert "word.Bet" not in verse
    assert "BetWith" not in verse
    # The empty <p></p> that separated the poetry lines from the heading is
    # gone, but did not merge anything together.
    assert "<p></p>" not in verse
