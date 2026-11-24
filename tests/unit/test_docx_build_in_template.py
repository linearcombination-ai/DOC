"""
The docx body is built straight into the rendered template (fast path) when
the template has, or can be given, the styles html4docx needs; otherwise a
separate body document is merged in with docxcompose (fallback). These tests
pin what the two paths must agree on, and show the kind of structure
assertions DOC's docx tests otherwise lack (they only check that a file exists).
"""

import hashlib
import json
import re
from pathlib import Path
from typing import cast

import pytest
from doc.domain import document_generator
from doc.domain.model import DocumentPart
from doc.utils import docx_util
from docx import Document
from docx.oxml.ns import qn
from lxml import etree

REPO = Path(__file__).resolve().parents[2]
TEMPLATE = str(REPO / "template.docx")
COMPACT_TEMPLATE = str(REPO / "template_compact.docx")

PARTS = [
    DocumentPart(content="<h1 class='book-name'>Translation Notes</h1>"),
    DocumentPart(
        content=(
            "<h2>Introduction</h2><h4>John 1:1</h4><h5>A heading</h5>"
            "<p>Text with a verse<sup>1</sup> marker and "
            "<a href='https://example.com/x'>an external link</a> and "
            "<a href='./01.md'>a relative note link</a>.</p>"
            "<ol><li>one</li><li>two</li></ol>"
            "<ul><li>a<ul><li>nested</li></ul></li><li>b</li></ul>"
        )
    ),
    DocumentPart(content="<p>Second one-column part</p>"),
    DocumentPart(
        content="<div style='column-count: 2;'><h4>John 1:2</h4><p>Notes</p></div>",
        contained_in_two_column_section=True,
    ),
    DocumentPart(
        content="<h2>Questions</h2><p>Q</p>", use_section_visual_separator=True
    ),
    DocumentPart(content="<p>The last part</p>"),
]


def build(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    name: str,
    template: str,
    fast: bool,
) -> Path:
    path = tmp_path / f"{name}.docx"
    with monkeypatch.context() as m:
        if not fast:  # pretend the styles cannot be provided: use the merge path
            m.setattr(
                document_generator,
                "add_styles_from_default_template",
                lambda doc, names: False,
            )
        document_generator.convert_html_to_docx(
            "unused.html",
            str(path),
            PARTS,
            template == COMPACT_TEMPLATE,
            "Title one",
            "Title two",
            docx_template_path=TEMPLATE,
            docx_compact_template_path=COMPACT_TEMPLATE,
        )
    return path


def structure(path: Path) -> dict[str, object]:
    """What a person sees as structure, independent of element ids."""
    doc = Document(str(path))
    body = doc.element.body
    numbering_xml = doc.part.numbering_part.element

    def list_definition(num_id: str) -> str:
        """The list a numId means: its abstractNum without ids, plus overrides."""
        num = numbering_xml.xpath(f'.//w:num[@w:numId="{num_id}"]')[0]
        abstract_id = num.xpath("string(.//w:abstractNumId/@w:val)")
        abstract = numbering_xml.xpath(
            f'.//w:abstractNum[@w:abstractNumId="{abstract_id}"]'
        )[0]
        text = re.sub(
            r'<w:(nsid|tmpl) [^>]*/>|w:abstractNumId="\d+"',
            "",
            etree.tostring(abstract).decode(),
        ) + "".join(etree.tostring(o).decode() for o in num.xpath("./w:lvlOverride"))
        return hashlib.sha1(text.encode()).hexdigest()[:8]

    numbering = {
        n.get(qn("w:numId")): list_definition(n.get(qn("w:numId")))
        for n in numbering_xml.xpath(".//w:num")
    }
    paragraphs = []
    for p in doc.paragraphs:
        num_id = p._p.xpath("string(./w:pPr/w:numPr/w:numId/@w:val)")
        paragraphs.append(
            (
                p.text,
                p.style.name if p.style is not None else "",
                numbering.get(num_id, ""),
                bool(p._p.xpath("./w:pPr/w:sectPr")),
            )
        )
    sections = [
        (
            s.start_type,
            s._sectPr.xpath("string(./w:cols/@w:num)"),
            s.left_margin,
            s.right_margin,
            s.top_margin,
            s.bottom_margin,
        )
        for s in doc.sections
    ]
    links = [
        (h.get(qn("w:anchor")), "".join(h.itertext()))
        for h in body.iter(qn("w:hyperlink"))
    ]
    styles = sorted(
        {
            s.xpath("string(./w:name/@w:val)")
            for s in doc.styles.element.xpath("./w:style")
        }
    )
    return {
        "paragraphs": paragraphs,
        "sections": sections,
        "links": links,
        "styles": styles,
        "superscripts": len(body.xpath(".//w:vertAlign[@w:val='superscript']")),
        "drawings": len(body.xpath(".//w:drawing")),
        "toc": body.xpath("string(.//w:instrText)").strip(),
    }


def test_styles_needed_for() -> None:
    needed = document_generator.styles_needed_for(PARTS)
    assert needed == [
        "Heading 2",
        "Heading 4",
        "Heading 5",
        "List Bullet",
        "List Bullet 2",
        "List Number",
        "Title",
    ]


def test_has_internal_link_markers() -> None:
    assert not docx_util.has_internal_link_markers(p.content for p in PARTS)
    assert docx_util.has_internal_link_markers(["x {{LINK_START:a}}y{{LINK_END}}"])
    assert docx_util.has_internal_link_markers(["{{BOOKMARK:a}}"])


@pytest.mark.parametrize("template", [TEMPLATE, COMPACT_TEMPLATE])
def test_fast_path_matches_merge_path(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, template: str
) -> None:
    fast = structure(build(tmp_path, monkeypatch, "fast", template, fast=True))
    merged = structure(build(tmp_path, monkeypatch, "merged", template, fast=False))
    assert fast == merged


@pytest.mark.parametrize("template", [TEMPLATE, COMPACT_TEMPLATE])
def test_structure_of_output(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, template: str
) -> None:
    s = structure(build(tmp_path, monkeypatch, "fast", template, fast=True))
    paragraphs = cast(list[tuple[str, str, str, bool]], s["paragraphs"])
    texts = [p[0] for p in paragraphs]
    # front matter comes from the template, content follows, in order
    assert texts[0] == "Title one"
    assert texts.index("Translation Notes") < texts.index("The last part")
    styles = {p[0]: p[1] for p in paragraphs}
    assert styles["Translation Notes"] == "Title"
    assert styles["John 1:1"] == "Heading 4"
    # one TOC field, hyperlinks kept, verse markers superscript, one rule drawing
    # beyond the template's own
    assert cast(str, s["toc"]).startswith("TOC")
    assert len(cast(list[object], s["links"])) >= 3
    assert s["superscripts"] == 1
    template_drawings = len(Document(template).element.body.xpath(".//w:drawing"))
    assert s["drawings"] == template_drawings + 1
    # the two-column part is the only two-column section
    columns = [c[1] for c in cast(list[tuple[object, str]], s["sections"])]
    assert columns.count("2") == 1
    # the first numbered item restarts at 1: it has its own w:num with an override
    doc = Document(str(tmp_path / "fast.docx"))
    assert doc.part.numbering_part.element.xpath(
        ".//w:num[w:lvlOverride/w:startOverride[@w:val='1']]"
    )


def test_bookmark_and_drawing_ids_are_unique(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    doc = Document(str(build(tmp_path, monkeypatch, "fast", TEMPLATE, fast=True)))
    ids = doc.element.body.xpath(".//wp:docPr/@id")
    assert len(ids) == len(set(ids))
    marker = json.dumps(ids)  # keep the list in the failure message
    assert marker
