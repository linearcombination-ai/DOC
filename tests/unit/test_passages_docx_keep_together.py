from pathlib import Path
from typing import Optional

from docx import Document
from docx.document import Document as DocxDocument
from docx.table import _Cell, _Row
from docx.text.paragraph import Paragraph
from docx.oxml.ns import qn
from passages.domain.document_generator import generate_docx
from passages.domain.model import Passage
from passages.reviewers_guide.model import BibleReference


def _passage(reference: str, html: str, is_available: bool = True) -> Passage:
    return Passage(
        reference=BibleReference(
            lang_code="en",
            book_code="gen",
            book_name="Genesis",
            start_chapter=1,
            start_chapter_verse_ref="1",
            end_chapter=None,
            end_chapter_verse_ref=None,
        ),
        passage_text=html,
        localized_reference=reference,
        is_available=is_available,
    )


def _verse(n: int) -> str:
    return (
        f'<span class="verse"><sup class="versemarker">{n}</sup>Verse {n} text. </span>'
    )


def _poetry(n_lines: int) -> str:
    lines = "".join(
        f'<div class="poetry-1">Line {i} of poem</div>' for i in range(n_lines)
    )
    return f'<span class="verse"><sup class="versemarker">1</sup>{lines}</span>'


def _generate(
    tmp_path: Path,
    lang0: list[Passage],
    lang1: Optional[list[Passage]] = None,
) -> DocxDocument:
    path = str(tmp_path / "out.docx")
    generate_docx(
        lang0,
        lang1 or [],
        path,
        "en",
        "English",
        "fr" if lang1 else None,
        "French" if lang1 else None,
    )
    return Document(path)


def _has(paragraph: Paragraph, tag: str) -> bool:
    return paragraph._p.pPr is not None and paragraph._p.pPr.find(qn(tag)) is not None


def _cant_split(row: _Row) -> bool:
    tr_pr = row._tr.find(qn("w:trPr"))
    return tr_pr is not None and tr_pr.find(qn("w:cantSplit")) is not None


def _passage_paragraphs(cell: _Cell, reference: str) -> list[Paragraph]:
    """Paragraphs from the reference line to the end of the cell"""
    texts = [p.text for p in cell.paragraphs]
    paragraphs: list[Paragraph] = cell.paragraphs
    return paragraphs[texts.index(reference) :]


def _assert_kept(cell: _Cell, reference: str) -> None:
    paragraphs = _passage_paragraphs(cell, reference)
    assert paragraphs[0].text == reference
    for p in paragraphs:
        assert _has(p, "w:keepLines")
    for p in paragraphs[:-1]:
        assert _has(p, "w:keepNext")
    assert not _has(paragraphs[-1], "w:keepNext")


def test_single_language_multi_paragraph_passage(tmp_path: Path) -> None:
    doc = _generate(tmp_path, [_passage("Psalm 1:1-2", _poetry(3))])
    row = doc.tables[0].rows[0]
    assert _cant_split(row)
    cell = row.cells[0]
    _assert_kept(cell, "Psalm 1:1-2")
    # The reference line is glued to the passage text
    assert len(_passage_paragraphs(cell, "Psalm 1:1-2")) >= 4


def test_leading_empty_paragraph_untouched(tmp_path: Path) -> None:
    doc = _generate(tmp_path, [_passage("Gen 1:1", _verse(1))])
    cell = doc.tables[0].rows[0].cells[0]
    assert cell.paragraphs[0].text == ""
    assert not _has(cell.paragraphs[0], "w:keepLines")
    assert not _has(cell.paragraphs[0], "w:keepNext")


def test_short_multi_passage_rows_each_kept_but_not_chained(tmp_path: Path) -> None:
    """Several short passages: each row is kept whole, but nothing glues
    one passage to the next, so several can share a page."""
    passages = [_passage(f"Gen 1:{n}", _verse(n)) for n in range(1, 6)]
    doc = _generate(tmp_path, passages)
    rows = doc.tables[0].rows
    assert len(rows) == 5
    for n, row in enumerate(rows, start=1):
        assert _cant_split(row)
        _assert_kept(row.cells[0], f"Gen 1:{n}")
        # Last paragraph of the passage must not be glued to the next row
        assert not _has(row.cells[0].paragraphs[-1], "w:keepNext")


def test_two_language_rows_track_each_cell_separately(tmp_path: Path) -> None:
    lang0 = [_passage("Psalm 1:1-2", _poetry(2)), _passage("Gen 1:1", _verse(1))]
    lang1 = [_passage("Psaume 1:1-2", _poetry(4)), _passage("Gen 1:1 fr", _verse(1))]
    doc = _generate(tmp_path, lang0, lang1)
    rows = doc.tables[0].rows
    for row, (ref0, ref1) in zip(
        rows, [("Psalm 1:1-2", "Psaume 1:1-2"), ("Gen 1:1", "Gen 1:1 fr")]
    ):
        assert _cant_split(row)
        _assert_kept(row.cells[0], ref0)
        _assert_kept(row.cells[1], ref1)


def test_unavailable_passage_reference_only(tmp_path: Path) -> None:
    doc = _generate(tmp_path, [_passage("Gen 1:1", "", is_available=False)])
    row = doc.tables[0].rows[0]
    assert _cant_split(row)
    paragraphs = _passage_paragraphs(row.cells[0], "Gen 1:1")
    assert len(paragraphs) == 1
    assert _has(paragraphs[0], "w:keepLines")
    assert not _has(paragraphs[0], "w:keepNext")


def test_passage_longer_than_a_page_does_not_error(tmp_path: Path) -> None:
    """A passage taller than a page still gets the properties; Word will
    split it where it must. Generation must succeed."""
    long_passage = _passage("Exodus 14:1-31", _poetry(400))
    short_passage = _passage("Gen 1:1", _verse(1))
    doc = _generate(tmp_path, [long_passage, short_passage])
    rows = doc.tables[0].rows
    assert len(rows) == 2
    for row, ref in zip(rows, ["Exodus 14:1-31", "Gen 1:1"]):
        assert _cant_split(row)
        _assert_kept(row.cells[0], ref)
    assert len(_passage_paragraphs(rows[0].cells[0], "Exodus 14:1-31")) > 400
