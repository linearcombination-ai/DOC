from __future__ import annotations

import re
from typing import Optional, cast, TYPE_CHECKING

from docx import Document
from docx.document import Document as DocxDocument
from docx.enum.section import WD_SECTION
from docx.enum.text import WD_ALIGN_PARAGRAPH, WD_PARAGRAPH_ALIGNMENT
from docx.oxml import OxmlElement
from docx.oxml.ns import qn
from docx.oxml.table import CT_Tc
from docx.shared import Pt, RGBColor
from docx.table import Table, _Cell, _Row
from docx.text.paragraph import Paragraph
from html4docx import HtmlToDocx  # type: ignore[import-untyped]


if TYPE_CHECKING:
    from typing import TypeAlias

    Cell: TypeAlias = _Cell
    Row: TypeAlias = _Row
else:
    Cell = _Cell
    Row = _Row


def format_docx_tables(doc: DocxDocument) -> DocxDocument:
    for table in doc.tables:
        tbl = table._element
        tbl_borders = OxmlElement("w:tblBorders")
        for name in ("top", "left", "bottom", "right", "insideH", "insideV"):
            border = OxmlElement(f"w:{name}")
            border.set(qn("w:val"), "single")
            border.set(qn("w:sz"), "8")  # 1px equivalent in Word (1/8 point units)
            border.set(qn("w:space"), "0")
            border.set(qn("w:color"), "000000")  # Border color
            tbl_borders.append(border)
        tbl.tblPr.append(tbl_borders)
        # Ensure text in each cell is vertically centered and free of
        # excessive space.
        for row in table.rows:
            for cell in row.cells:
                tc = cell._tc
                tc_pr = tc.get_or_add_tcPr()
                v_align = OxmlElement("w:vAlign")
                v_align.set(qn("w:val"), "center")
                tc_pr.append(v_align)
                para = cell.paragraphs[0]
                para.paragraph_format.space_after = Pt(0)
    return doc


def add_checkbox_column(docx_filepath: str) -> None:
    """
    Add a third column to each table in a DOCX file, with each cell in the new column containing an unchecked checkbox.
    """
    doc = Document(docx_filepath)
    for table in doc.tables:
        add_column_with_checkboxes(table)
    doc.save(docx_filepath)


def add_column_with_checkboxes(table: Table) -> None:
    """
    Add a new column to the right of a table, with each cell containing an unchecked checkbox.
    """
    for row in table.rows:
        cell = _append_cell(row)
        if cell is not None:
            _add_checkbox(cell)


def _append_cell(row: Row) -> Optional[Cell]:
    tc = OxmlElement("w:tc")
    tc.append(OxmlElement("w:tcPr"))
    row._tr.append(tc)
    return Cell(cast(CT_Tc, tc), row.table)


def _add_checkbox(cell: Cell) -> None:
    checkbox = OxmlElement("w:sdt")
    sdt_pr = OxmlElement("w:sdtPr")
    sdt_pr.append(OxmlElement("w:checkBox"))
    checkbox.append(sdt_pr)
    sdt_content = OxmlElement("w:sdtContent")
    p = OxmlElement("w:p")
    r = OxmlElement("w:r")
    t = OxmlElement("w:t")
    t.text = "☐"
    r.append(t)
    p.append(r)
    sdt_content.append(p)
    checkbox.append(sdt_content)
    cell._tc.append(checkbox)


def add_header(
    doc: DocxDocument,
    source_lang_code: str,
    target_lang_code: str,
    header_text: str = "Spiritual Terms Evaluation Tool (STET)",
) -> DocxDocument:
    section = doc.sections[0]
    header = section.header
    para = header.add_paragraph()
    run1 = para.add_run(header_text)
    run1.font.size = Pt(12)
    run1.font.color.rgb = RGBColor(169, 169, 169)
    para.add_run("\t")
    run2 = para.add_run(f"{source_lang_code.upper()}/{target_lang_code.upper()}")
    run2.font.color.rgb = RGBColor(169, 169, 169)
    assert section.page_width is not None
    section.left_margin = section.right_margin = Pt(72)  # 1-inch margins
    section.top_margin = section.bottom_margin = Pt(72)
    usable_width = section.page_width - section.left_margin - section.right_margin
    tab_pos = int(section.left_margin + usable_width * 0.75)
    para.paragraph_format.tab_stops.add_tab_stop(
        tab_pos, alignment=WD_ALIGN_PARAGRAPH.RIGHT
    )
    para.alignment = WD_ALIGN_PARAGRAPH.LEFT
    return doc


def add_highlighted_html_to_docx_for_words(
    html: str, paragraph: Paragraph, keywords: list[str]
) -> None:
    """
    Convert HTML to DOCX and highlight occurrences of keywords in bold.
    :param html: The HTML string to convert.
    :param paragraph: The DOCX paragraph where the content will be added.
    :param keywords: The list of keywords to highlight in bold.
    """
    temp_doc = Document()
    HtmlToDocx().add_html_to_document(html, temp_doc)
    regex = re.compile(
        r"\b(" + "|".join(re.escape(k) for k in keywords) + r")\b",
        re.IGNORECASE,
    )
    for p in temp_doc.paragraphs:
        text = p.text
        pos = 0
        for m in regex.finditer(text):
            if m.start() > pos:
                paragraph.add_run(text[pos : m.start()])
            run = paragraph.add_run(m.group(0))
            run.bold = True
            pos = m.end()
        if pos < len(text):
            paragraph.add_run(text[pos:])


def add_plain_html_to_docx(html: str, paragraph: Paragraph) -> None:
    """
    Convert HTML to DOCX without highlighting.

    :param html: The HTML string to convert.
    :param paragraph: The DOCX paragraph where content will be added.
    """
    # Use HtmlToDocx to convert the HTML to the target paragraph
    html_to_docx = HtmlToDocx()
    temp_doc = Document()
    html_to_docx.add_html_to_document(html, temp_doc)
    # Add plain text from the temp_doc into the target paragraph
    for temp_paragraph in temp_doc.paragraphs:
        paragraph.add_run(temp_paragraph.text.strip())


def add_preformatted_html_to_docx(html: str, paragraph: Paragraph) -> None:
    """
    Convert HTML with <b> tags to DOCX, preserving bold formatting.
    Used when source text comes from fully specified <r><v> format.

    :param html: The HTML string to convert (may contain <b> tags).
    :param paragraph: The DOCX paragraph where content will be added.
    """
    html_to_docx = HtmlToDocx()
    temp_doc = Document()
    html_to_docx.add_html_to_document(html, temp_doc)
    for temp_paragraph in temp_doc.paragraphs:
        for run in temp_paragraph.runs:
            new_run = paragraph.add_run(run.text)
            new_run.bold = bool(run.bold)


def add_lined_page_at_end(doc: DocxDocument) -> DocxDocument:
    """
    Adds a single page filled with ruled lines to the end of the document for note-taking.
    Each line spans the full page width and is evenly spaced.
    :param doc: The Word document to which the ruled page will be added.
    :return: The modified Word document.
    """
    section = doc.add_section(
        start_type=WD_SECTION.NEW_PAGE
    )  # Add a new section for a new page
    section.left_margin = section.right_margin = Pt(72)  # 1-inch margins
    section.top_margin = section.bottom_margin = Pt(72)
    usable_height = section.page_height - section.top_margin - section.bottom_margin
    line_spacing = Pt(18)  # Approx. 1.5x line spacing for handwriting clarity
    num_lines = int(usable_height / line_spacing)
    # Add a single paragraph with blank lines separated by line breaks
    lined_paragraph = doc.add_paragraph()
    lined_paragraph.alignment = WD_PARAGRAPH_ALIGNMENT.LEFT
    lined_paragraph.paragraph_format.space_before = Pt(0)
    lined_paragraph.paragraph_format.space_after = Pt(0)
    lined_paragraph.paragraph_format.line_spacing = line_spacing
    for _ in range(num_lines - 3):
        lined_paragraph.add_run("_" * 100)  # Add a visible placeholder for each line
        lined_paragraph.add_run("\n")  # Add a line break to simulate ruled lines
    return doc


def adjust_table_columns(table: Table) -> None:
    """
    Adjusts the table columns so that the last column ('Status') is minimal,
    while the other columns take up the remaining space.
    :param table: The table to adjust.
    """
    # Set widths for each column
    column_widths = [5.5, 5.5, 1.0]  # Adjust widths in inches (example values)
    # Apply widths to the columns
    for col_idx, width in enumerate(column_widths):
        for cell in table.columns[col_idx].cells:
            tc = cell._tc
            tcPr = tc.get_or_add_tcPr()
            tcW = OxmlElement("w:tcW")
            tcW.set(qn("w:w"), str(int(width * 1440)))  # Convert inches to twips
            tcW.set(qn("w:type"), "dxa")
            tcPr.append(tcW)


def reduce_spacing_around_tables(
    doc: DocxDocument, before_table_space: int = 0, after_table_space: int = 0
) -> None:
    """
    Reduces the whitespace around tables in a Word document.

    Parameters:
        doc (Document): A `Document` instance from python-docx.
        before_table_space (int): The spacing (in points) to set before a table. Default is 0.
        after_table_space (int): The spacing (in points) to set after a table. Default is 0.
    """

    def set_spacing(
        paragraph: Paragraph, before: Optional[int] = None, after: Optional[int] = None
    ) -> None:
        # Access or create the <w:spacing> element
        pPr = paragraph._element.get_or_add_pPr()
        spacing = pPr.find(qn("w:spacing"))
        if spacing is None:
            spacing = OxmlElement("w:spacing")
            pPr.append(spacing)
        if before is not None:
            spacing.set(qn("w:before"), str(before))
        if after is not None:
            spacing.set(qn("w:after"), str(after))

    # Iterate through all elements in the document
    previous_element = None
    for element in doc.element.body:
        if element.tag.endswith("tbl"):  # Table tag
            # If there's a previous element, adjust its spacing after the element
            if previous_element is not None and previous_element.tag.endswith("p"):
                paragraph = Paragraph(previous_element, doc)
                set_spacing(paragraph, after=before_table_space)
            previous_element = element
        elif element.tag.endswith("p"):  # Paragraph tag
            paragraph = Paragraph(element, doc)
            if previous_element is not None and previous_element.tag.endswith("tbl"):
                # Adjust spacing for the paragraph following a table
                set_spacing(paragraph, before=after_table_space)
            previous_element = element


def add_footer(doc: DocxDocument, date_text: str) -> DocxDocument:
    section = doc.sections[0]
    footer = section.footer
    para = footer.paragraphs[0] if footer.paragraphs else footer.add_paragraph()
    para.alignment = None
    p_pr = para._p.get_or_add_pPr()
    tabs = p_pr.find(qn("w:tabs")) or OxmlElement("w:tabs")
    p_pr.append(tabs)
    assert section.page_width is not None
    assert section.left_margin is not None
    assert section.right_margin is not None
    usable_width = section.page_width - section.left_margin - section.right_margin

    def _tab(val: str, pos: int) -> None:
        t = OxmlElement("w:tab")
        t.set(qn("w:val"), val)
        t.set(qn("w:pos"), str(pos))
        tabs.append(t)

    _tab("center", int(usable_width / 2))
    _tab("right", int(usable_width - 720))
    para.add_run("\t")
    fld = OxmlElement("w:fldSimple")
    fld.set(qn("w:instr"), "PAGE")
    para.add_run()._r.append(fld)
    para.add_run("\t")
    r = para.add_run(date_text)
    r.font.color.rgb = RGBColor(169, 169, 169)
    r.font.size = Pt(10)
    return doc
