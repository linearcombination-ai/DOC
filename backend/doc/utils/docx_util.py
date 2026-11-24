import re
from collections.abc import Iterable
from copy import deepcopy
from pathlib import Path
from typing import cast

from docx import Document
from docx.document import Document as DocxDocument
from docx.enum.style import WD_STYLE_TYPE
from docx.opc.constants import RELATIONSHIP_TYPE
from docx.oxml import OxmlElement, parse_xml
from docx.oxml.ns import qn
from docx.shared import RGBColor
from docx.text.paragraph import Paragraph
from docx.text.run import Run
from lxml.etree import _Element as Element


def generate_docx_toc(docx_filepath: str) -> str:
    """
    Create subdocument that contains only the code to generate (on
    first open of document) the table of contents.
    """
    toc_path = f"{Path(docx_filepath).with_suffix('')}_toc.docx"
    document = Document()
    paragraph = document.add_paragraph()
    run = paragraph.add_run()
    fldChar = OxmlElement("w:fldChar")
    fldChar.set(qn("w:fldCharType"), "begin")
    instrText = OxmlElement("w:instrText")
    instrText.set(qn("xml:space"), "preserve")
    instrText.text = (
        r'TOC \o "1-2" \h \z \u'  # change 1-2 depending on heading levels you need
    )
    fldChar2 = OxmlElement("w:fldChar")
    fldChar2.set(qn("w:fldCharType"), "separate")
    fldChar3 = OxmlElement("w:t")
    fldChar3.text = (
        "Right-click to update field (doing so will insert table of contents)."
    )
    fldChar2.append(fldChar3)
    fldChar4 = OxmlElement("w:fldChar")
    fldChar4.set(qn("w:fldCharType"), "end")
    r_element = run._r
    r_element.append(fldChar)
    r_element.append(instrText)
    r_element.append(fldChar2)
    r_element.append(fldChar4)
    document.save(toc_path)
    return str(toc_path)


def _make_text_run(text: str) -> Element:
    r = OxmlElement("w:r")
    t = OxmlElement("w:t")
    t.text = text
    # Ensure Word preserves leading/trailing spaces
    t.set("{http://www.w3.org/XML/1998/namespace}space", "preserve")
    r.append(t)
    return r


def override_and_clean_hyperlinks(
    doc: DocxDocument, style_id: str, first_paragraph: int = 0
) -> None:
    """
    Finds all hyperlinks, strips any forced inline formatting (like blue color),
    and applies the correct template character style ID using raw XML manipulation.
    Paragraphs before index first_paragraph are left alone.
    """
    for paragraph in doc.paragraphs[first_paragraph:]:
        hyperlinks = paragraph._p.findall(
            ".//{http://schemas.openxmlformats.org/wordprocessingml/2006/main}hyperlink"
        )
        for hyperlink in hyperlinks:
            runs = hyperlink.findall(qn("w:r"))
            for r in runs:
                rPr = r.find(qn("w:rPr"))
                if rPr is None:
                    rPr = OxmlElement("w:rPr")
                    r.insert(0, rPr)
                existing_style = rPr.find(qn("w:rStyle"))
                if existing_style is not None:
                    rPr.remove(existing_style)
                for child in list(rPr):
                    if child.tag in (qn("w:color"), qn("w:u"), qn("w:shd")):
                        rPr.remove(child)
                rStyle = OxmlElement("w:rStyle")
                rStyle.set(qn("w:val"), style_id)
                rPr.insert(0, rStyle)


def _make_internal_hyperlink_element(text: str, bookmark_name: str) -> Element:
    hyperlink = OxmlElement("w:hyperlink")
    hyperlink.set(qn("w:anchor"), bookmark_name)
    hyperlink.set(qn("w:history"), "1")
    r = OxmlElement("w:r")
    r_pr = OxmlElement("w:rPr")
    r_style = OxmlElement("w:rStyle")
    r_style.set(qn("w:val"), "Hyperlink")
    r_pr.append(r_style)
    r.append(r_pr)
    t = OxmlElement("w:t")
    t.text = text
    t.set("{http://www.w3.org/XML/1998/namespace}space", "preserve")
    r.append(t)
    hyperlink.append(r)
    return hyperlink


def _add_bookmark_to_run(run: Run, bookmark_name: str) -> None:
    """Add a DOCX bookmark around the given run in-place."""
    r = run._r
    p = r.getparent()
    if p is None:
        return
    start = OxmlElement("w:bookmarkStart")
    start.set(qn("w:id"), "0")
    start.set(qn("w:name"), bookmark_name)
    end = OxmlElement("w:bookmarkEnd")
    end.set(qn("w:id"), "0")
    idx = p.index(r)
    p.insert(idx, start)
    p.insert(idx + 1, end)


def _replace_runs(para: Paragraph, new_elems: list[Element]) -> None:
    """
    Replace all runs in a paragraph with the provided XML elements.
    (Keeps paragraph element intact and appends supplied elements.)
    """
    for run in list(para.runs):
        p_r = run._r
        p = p_r.getparent()
        if p is not None and p_r in p:
            p.remove(p_r)
    for elem in new_elems:
        para._p.append(elem)


def has_internal_link_markers(contents: Iterable[str]) -> bool:
    """
    True if any HTML fragment contains a marker that add_internal_docx_links
    would convert. Nothing in the backend emits these markers today, so
    callers can skip that (slow) pass when this is False.
    """
    return any("{{BOOKMARK:" in c or "{{LINK_START:" in c for c in contents)


def add_internal_docx_links(doc: DocxDocument) -> None:
    """
    Convert {{BOOKMARK:name}} markers into bookmarks, and
    convert {{LINK_START:name}}...{{LINK_END}} sequences into internal links.
    Operates paragraph-by-paragraph using paragraph text aggregated
    from runs so punctuation/spaces are preserved.
    """
    bookmark_map: dict[str, str] = {}
    # Pass 1 — find and create bookmarks inside runs (remove marker text)
    bookmark_pattern = re.compile(r"\{\{BOOKMARK:([^}]+)\}\}")
    for para in doc.paragraphs:
        for run in para.runs:
            m = bookmark_pattern.search(run.text)
            if not m:
                continue
            name = m.group(1)
            # remove marker from run text
            run.text = bookmark_pattern.sub("", run.text).strip()
            # add bookmark around this run
            _add_bookmark_to_run(run, name)
            bookmark_map[name] = name
    # Pass 2 — replace LINK_START/LINK_END sequences at paragraph level
    # pattern matches sequences like {{LINK_START:name}}...{{LINK_END}}
    link_pattern = re.compile(
        r"\{\{LINK_START:([^}]+)\}\}(.*?)\{\{LINK_END\}\}", flags=re.DOTALL
    )
    for para in doc.paragraphs:
        # combine paragraph text from runs to preserve exact punctuation/spacing
        combined_text = "".join(run.text for run in para.runs)
        if "{{LINK_START:" not in combined_text:
            # nothing to do for this paragraph
            continue
        new_elements: list[Element] = []
        cursor = 0
        for m in link_pattern.finditer(combined_text):
            start, end = m.span()
            target = m.group(1)
            link_text = m.group(2)
            # add literal text before this link (commas, spaces, etc.)
            if start > cursor:
                literal = combined_text[cursor:start]
                if literal:
                    new_elements.append(_make_text_run(literal))
            # add hyperlink element or fallback to plain text if bookmark missing
            if target in bookmark_map:
                new_elements.append(_make_internal_hyperlink_element(link_text, target))
            else:
                new_elements.append(_make_text_run(link_text))
            cursor = end
        # trailing text after last link
        if cursor < len(combined_text):
            tail = combined_text[cursor:]
            if tail:
                new_elements.append(_make_text_run(tail))
        # replace the paragraph's runs with our constructed elements
        if new_elements:
            _replace_runs(para, new_elements)


def style_superscripts(
    doc: DocxDocument,
    *,
    lift_half_points: int = 2,
    color: RGBColor | None = None,
    first_paragraph: int = 0,
) -> None:
    """
    lift_half_points:
        2 = +1pt
        4 = +2pt
        6 = +3pt
    color:
        RGBColor for superscripts (e.g. light gray) or None
    first_paragraph:
        index of the first paragraph to process; lets callers skip
        paragraphs that were in the document before the content was added.
    """
    for para in doc.paragraphs[first_paragraph:]:
        runs = para.runs
        for i, run in enumerate(runs):
            if run.font.superscript:
                # --- Ensure space before superscript ---
                if run.text and not run.text[0].isspace():
                    prev_char = None
                    if i > 0 and runs[i - 1].text:
                        prev_char = runs[i - 1].text[-1]
                    if not prev_char or not prev_char.isspace():
                        run.text = " " + run.text
                # --- Color ---
                run.font.color.rgb = color
                # --- Vertical position ---
                rPr = run._r.get_or_add_rPr()
                position = OxmlElement("w:position")
                position.set(qn("w:val"), str(lift_half_points))
                rPr.append(position)


def _ensure_character_style_based_on_default(
    doc: DocxDocument,
    name: str,
    *,
    color: RGBColor | None = None,
    italic: bool | None = None,
) -> None:
    styles = doc.styles
    if name in styles:
        return
    style = styles.add_style(name, WD_STYLE_TYPE.CHARACTER)
    # ---- Base on Default Paragraph Font ----
    style_elm = style._element
    based_on = OxmlElement("w:basedOn")
    based_on.set(qn("w:val"), "DefaultParagraphFont")
    style_elm.insert(0, based_on)
    # ---- Only override what is explicitly requested ----
    font = style.font
    if color is not None:
        font.color.rgb = color
    if italic is not None:
        font.italic = italic


def ensure_reference_styles(
    doc: DocxDocument,
    *,
    available_color: RGBColor | None = None,
    unavailable_color: RGBColor,
) -> None:
    """
    Create semantic character styles for passage references.

    AvailableReference:
        - Based on Default Paragraph Font
        - Usually no overrides (inherits document defaults)

    UnavailableReference:
        - Based on Default Paragraph Font
        - Lighter color + italics to signal intentional absence
    """
    _ensure_character_style_based_on_default(
        doc,
        "AvailableReference",
        color=available_color,  # usually None
    )
    _ensure_character_style_based_on_default(
        doc,
        "UnavailableReference",
        color=unavailable_color,
        italic=True,
    )


_W_NS = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
_WP_NS = "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"
_PIC_NS = "http://schemas.openxmlformats.org/drawingml/2006/picture"


def restart_first_list_numbering(doc: DocxDocument, first_element: int) -> None:
    """
    Make the first numbered paragraph of each list style (at or after body
    element index first_element) restart at 1, and keep the paragraphs that
    shared its numId on the restarted list. Same effect on the output as
    docxcompose's Composer.restart_first_numbering, which the old
    template-plus-body merge applied to the body.
    """
    numbering = doc.part.numbering_part.element
    done: set[str] = set()
    remapped: dict[str, str] = {}
    for element in list(doc.element.body)[first_element:]:
        p_pr = element.find(qn("w:pPr"))
        if p_pr is None:
            continue
        local_el = p_pr.find(qn("w:numPr") + "/" + qn("w:numId"))
        local = [local_el.get(qn("w:val"))] if local_el is not None else []
        if local_el is not None and local[0] in remapped:
            local = [remapped[local[0]]]
            local_el.set(qn("w:val"), local[0])
        style_el = p_pr.find(qn("w:pStyle"))
        if style_el is None:
            continue
        style_id = cast(str, style_el.get(qn("w:val")))
        if style_id in done:
            continue
        style = doc.styles.element.get_by_id(style_id)
        if style is None or style.xpath(".//w:outlineLvl"):
            continue  # headings never restart
        style_nums = style.xpath(".//w:numId/@w:val")
        num_id = local[0] if local else (style_nums[0] if style_nums else None)
        if num_id is None:
            continue
        nums = numbering.xpath(f'.//w:num[@w:numId="{num_id}"]')
        if not nums:
            continue
        abstract_id = nums[0].xpath(".//w:abstractNumId/@w:val")[0]
        fmt = numbering.xpath(
            f'.//w:abstractNum[@w:abstractNumId="{abstract_id}"]'
            '//w:lvl[@w:ilvl="0"]/w:numFmt/@w:val'
        )
        if fmt and fmt[0] == "bullet":
            continue
        restarted = deepcopy(nums[0])
        restarted.append(
            parse_xml(
                f'<w:lvlOverride xmlns:w="{_W_NS}" w:ilvl="0">'
                '<w:startOverride w:val="1"/></w:lvlOverride>'
            )
        )
        new_id = str(max(int(n) for n in numbering.xpath(".//w:num/@w:numId")) + 1)
        restarted.set(qn("w:numId"), new_id)
        last_num = numbering.xpath(".//w:num")[-1]
        numbering.insert(numbering.index(last_num), restarted)
        if local:
            remapped[local[0]] = new_id
            local_el.set(qn("w:val"), new_id)
        else:
            p_pr.append(
                parse_xml(
                    f'<w:numPr xmlns:w="{_W_NS}"><w:ilvl w:val="0"/>'
                    f'<w:numId w:val="{new_id}"/></w:numPr>'
                )
            )
        done.add(style_id)


def renumber_bookmarks_and_drawing_ids(doc: DocxDocument) -> None:
    """
    Give bookmarks and drawings unique ids. html4docx restarts bookmark ids
    at 0 for every HTML fragment, and the horizontal-rule drawings all use
    the same id. Drawing ids continue into the header and footer parts, the
    way docxcompose numbers them.
    """
    body = doc.element.body
    for tag in ("w:bookmarkStart", "w:bookmarkEnd"):
        for n, el in enumerate(body.iter(qn(tag))):
            el.set(qn("w:id"), str(n))
    header_footer_roots = [
        rel.target_part.element
        for rel in doc.part.rels.values()
        if rel.reltype in (RELATIONSHIP_TYPE.HEADER, RELATIONSHIP_TYPE.FOOTER)
    ]
    for tag in (f"{{{_WP_NS}}}docPr", f"{{{_PIC_NS}}}cNvPr"):
        next_id = 1
        for root in [body, *header_footer_roots]:
            for el in root.iter(tag):
                el.set("id", str(next_id))
                next_id += 1


def add_styles_from_default_template(doc: DocxDocument, names: Iterable[str]) -> bool:
    """
    Add each named style that doc lacks by copying it, with its numbering
    definition and linked character style, from python-docx's default
    template (the one the old body document was built on). Returns False,
    leaving doc untouched, if a style is in neither document.
    """
    missing = [name for name in names if name not in doc.styles]
    default = Document()
    if any(name not in default.styles for name in missing):
        return False
    for name in missing:
        _copy_style(default, doc, default.styles[name].style_id)
    return True


def _copy_style(src: DocxDocument, dst: DocxDocument, style_id: str) -> None:
    src_style = src.styles.element.get_by_id(style_id)
    if src_style is None or dst.styles.element.get_by_id(style_id) is not None:
        return
    new_style = deepcopy(src_style)
    dst.styles.element.append(new_style)
    for num_id_el in new_style.xpath(".//w:numId"):
        num_id_el.set(
            qn("w:val"), _copy_numbering(src, dst, num_id_el.get(qn("w:val")))
        )
    for linked in new_style.xpath(".//w:link/@w:val"):
        _copy_style(src, dst, linked)


def _copy_numbering(src: DocxDocument, dst: DocxDocument, num_id: str) -> str:
    """Copy one w:num and its w:abstractNum into dst; return the new numId."""
    src_numbering = src.part.numbering_part.element
    dst_numbering = dst.part.numbering_part.element
    num = deepcopy(src_numbering.xpath(f'.//w:num[@w:numId="{num_id}"]')[0])
    abstract_id = num.xpath(".//w:abstractNumId/@w:val")[0]
    abstract = deepcopy(
        src_numbering.xpath(f'.//w:abstractNum[@w:abstractNumId="{abstract_id}"]')[0]
    )
    new_abstract_id = 1 + max(
        (int(n) for n in dst_numbering.xpath(".//w:abstractNum/@w:abstractNumId")),
        default=-1,
    )
    new_num_id = 1 + max(
        (int(n) for n in dst_numbering.xpath(".//w:num/@w:numId")), default=0
    )
    abstract.set(qn("w:abstractNumId"), str(new_abstract_id))
    nsid = abstract.find(qn("w:nsid"))
    if nsid is not None:  # a unique list id, so lists do not continue each other
        nsid.set(qn("w:val"), f"{0x5A000000 + new_abstract_id:08X}")
    num.set(qn("w:numId"), str(new_num_id))
    num.find(qn("w:abstractNumId")).set(qn("w:val"), str(new_abstract_id))
    nums = dst_numbering.xpath(".//w:num")
    if nums:
        dst_numbering.insert(dst_numbering.index(nums[0]), abstract)
        dst_numbering.insert(dst_numbering.index(nums[-1]) + 1, num)
    else:
        dst_numbering.append(abstract)
        dst_numbering.append(num)
    return str(new_num_id)
