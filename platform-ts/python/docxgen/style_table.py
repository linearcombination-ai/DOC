"""One explicit style table that can emit docx styles (and, below, the PDF CSS).

Pure data + string emitters: no python-docx, no template file, no docxtpl.
Everything here maps one-to-one onto OOXML (w:style) and CSS properties, so the same
table can be ported to any language that can write XML and a zip.
"""

from dataclasses import dataclass, field
from typing import Literal

Line = tuple[Literal["exact", "auto", "atLeast"], float]  # ("exact", 15.0) pt | ("auto", 1.08) x


@dataclass(frozen=True)
class Style:
    id: str
    name: str
    kind: Literal["paragraph", "character"] = "paragraph"
    based_on: str | None = "Normal"
    next: str | None = None
    css: str | None = None  # CSS selector for the PDF side (None: docx-only style)
    font: str | None = None  # family; None inherits
    size_pt: float | None = None
    bold: bool | None = None
    italic: bool | None = None
    underline: bool | None = None
    color: str | None = None  # "000000"
    align: Literal["left", "center", "right", "both"] | None = None
    before_pt: float | None = None
    after_pt: float | None = None
    line: Line | None = None
    left_in: float | None = None
    first_line_in: float | None = None
    hanging_in: float | None = None
    keep_next: bool = False
    keep_lines: bool = False
    outline: int | None = None
    list_kind: Literal["bullet", "number"] | None = None
    list_left_in: float = 0.25  # indent of the list text (hanging 0.25 in for the marker)
    contextual_spacing: bool = False
    tabs: tuple[tuple[str, int], ...] = ()  # (alignment, twips)
    q_format: bool = True
    vert_align: str | None = None  # "superscript" (character styles)
    keep_with_next_default: bool = False


@dataclass(frozen=True)
class Page:
    width_in: float = 8.5
    height_in: float = 11.0
    top_in: float = 1.0
    bottom_in: float = 1.0
    left_in: float = 1.0
    right_in: float = 1.0
    header_in: float = 0.5
    footer_in: float = 0.5


@dataclass(frozen=True)
class Profile:
    name: str
    font: str  # document default family
    size_pt: float
    line: Line
    after_pt: float
    styles: tuple[Style, ...]
    page: Page = field(default_factory=Page)


def tw(inches: float) -> int:
    return round(inches * 1440)


def hp(points: float) -> int:  # half-points
    return round(points * 2)


def _ppr(s: Style) -> str:
    x = ""
    if s.keep_next:
        x += "<w:keepNext/>"
    if s.keep_lines:
        x += "<w:keepLines/>"
    if s.list_kind:
        x += f'<w:numPr><w:numId w:val="{_num_id(s)}"/></w:numPr>'
    if s.tabs:
        x += "<w:tabs>" + "".join(f'<w:tab w:val="{a}" w:pos="{p}"/>' for a, p in s.tabs) + "</w:tabs>"
    sp = ""
    if s.before_pt is not None:
        sp += f' w:before="{round(s.before_pt * 20)}"'
    if s.after_pt is not None:
        sp += f' w:after="{round(s.after_pt * 20)}"'
    if s.line:
        rule, v = s.line
        sp += f' w:line="{round(v * 20) if rule != "auto" else round(v * 240)}" w:lineRule="{rule}"'
    if sp:
        x += f"<w:spacing{sp}/>"
    ind = ""
    if s.left_in is not None:
        ind += f' w:left="{tw(s.left_in)}"'
    if s.first_line_in is not None:
        ind += f' w:firstLine="{tw(s.first_line_in)}"'
    if s.hanging_in is not None:
        ind += f' w:hanging="{tw(s.hanging_in)}"'
    if ind:
        x += f"<w:ind{ind}/>"
    if s.contextual_spacing:
        x += "<w:contextualSpacing/>"
    if s.align:
        x += f'<w:jc w:val="{s.align}"/>'
    if s.outline is not None:
        x += f'<w:outlineLvl w:val="{s.outline}"/>'
    return f"<w:pPr>{x}</w:pPr>" if x else ""


def _rpr(s: Style) -> str:
    x = ""
    if s.font:
        x += f'<w:rFonts w:ascii="{s.font}" w:hAnsi="{s.font}" w:eastAsia="{s.font}" w:cs="{s.font}"/>'
    if s.bold is not None:
        x += "<w:b/><w:bCs/>" if s.bold else '<w:b w:val="0"/><w:bCs w:val="0"/>'
    if s.italic is not None:
        x += "<w:i/><w:iCs/>" if s.italic else '<w:i w:val="0"/><w:iCs w:val="0"/>'
    if s.color:
        x += f'<w:color w:val="{s.color}"/>'
    if s.size_pt is not None:
        x += f'<w:sz w:val="{hp(s.size_pt)}"/><w:szCs w:val="{hp(s.size_pt)}"/>'
    if s.underline:
        x += '<w:u w:val="single"/>'
    if s.vert_align:
        x += f'<w:vertAlign w:val="{s.vert_align}"/>'
    return f"<w:rPr>{x}</w:rPr>" if x else ""


_NUMS = {("bullet", 1): 1, ("number", 1): 2, ("bullet", 2): 3}


def _num_id(s: Style) -> int:
    assert s.list_kind
    return 3 if s.id.endswith("2") and s.list_kind == "bullet" else (1 if s.list_kind == "bullet" else 2)


def styles_xml(p: Profile) -> str:
    w = 'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"'
    out = [f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles {w}>']
    out.append(
        "<w:docDefaults><w:rPrDefault><w:rPr>"
        f'<w:rFonts w:ascii="{p.font}" w:hAnsi="{p.font}" w:eastAsia="{p.font}" w:cs="{p.font}"/>'
        f'<w:sz w:val="{hp(p.size_pt)}"/><w:szCs w:val="{hp(p.size_pt)}"/>'
        '<w:lang w:val="en-US" w:eastAsia="en-US" w:bidi="ar-SA"/></w:rPr></w:rPrDefault>'
        "<w:pPrDefault><w:pPr>"
        f'<w:spacing w:after="{round(p.after_pt * 20)}" w:line="{round(p.line[1] * 240)}" w:lineRule="auto"/>'
        "</w:pPr></w:pPrDefault></w:docDefaults>"
    )
    out.append(
        '<w:style w:type="character" w:default="1" w:styleId="DefaultParagraphFont">'
        '<w:name w:val="Default Paragraph Font"/><w:uiPriority w:val="1"/><w:semiHidden/><w:unhideWhenUsed/></w:style>'
        '<w:style w:type="table" w:default="1" w:styleId="TableNormal"><w:name w:val="Normal Table"/>'
        '<w:uiPriority w:val="99"/><w:semiHidden/><w:unhideWhenUsed/><w:tblPr><w:tblInd w:w="0" w:type="dxa"/>'
        '<w:tblCellMar><w:top w:w="0" w:type="dxa"/><w:left w:w="108" w:type="dxa"/><w:bottom w:w="0" w:type="dxa"/>'
        '<w:right w:w="108" w:type="dxa"/></w:tblCellMar></w:tblPr></w:style>'
        '<w:style w:type="numbering" w:default="1" w:styleId="NoList"><w:name w:val="No List"/>'
        '<w:uiPriority w:val="99"/><w:semiHidden/><w:unhideWhenUsed/></w:style>'
    )
    for s in p.styles:
        default = ' w:default="1"' if s.id == "Normal" else ""
        custom = ' w:customStyle="1"' if s.id in CUSTOM_IDS else ""
        x = f'<w:style w:type="{s.kind}"{default}{custom} w:styleId="{s.id}"><w:name w:val="{s.name}"/>'
        if s.based_on and s.id != "Normal":
            x += f'<w:basedOn w:val="{s.based_on}"/>'
        if s.next:
            x += f'<w:next w:val="{s.next}"/>'
        if s.q_format:
            x += "<w:qFormat/>"
        x += _ppr(s) + _rpr(s) + "</w:style>"
        out.append(x)
    out.append("</w:styles>")
    return "".join(out)


CUSTOM_IDS = {"LicenseText", "PlainHyperlink", "PlainHyperlinkChar0", "UlbText", "IndentedText"}


def numbering_xml() -> str:
    w = 'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"'

    def lvl(fmt: str, text: str, left: int, style: str, font: str = "") -> str:
        rpr = f'<w:rPr><w:rFonts w:ascii="{font}" w:hAnsi="{font}" w:hint="default"/></w:rPr>' if font else ""
        return (
            f'<w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="{fmt}"/>{f'<w:pStyle w:val="{style}"/>' if style else ""}'
            f'<w:lvlText w:val="{text}"/><w:lvlJc w:val="left"/><w:pPr><w:ind w:left="{left}" w:hanging="360"/></w:pPr>{rpr}</w:lvl>'
        )

    bullet = ""
    return (
        f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:numbering {w}>'
        f'<w:abstractNum w:abstractNumId="0"><w:nsid w:val="5A000000"/><w:multiLevelType w:val="singleLevel"/>{lvl("bullet", bullet, 360, "ListBullet", "Symbol")}</w:abstractNum>'
        f'<w:abstractNum w:abstractNumId="1"><w:nsid w:val="5A000001"/><w:multiLevelType w:val="singleLevel"/>{lvl("decimal", "%1.", 360, "ListNumber")}</w:abstractNum>'
        f'<w:abstractNum w:abstractNumId="2"><w:nsid w:val="5A000002"/><w:multiLevelType w:val="singleLevel"/>{lvl("bullet", bullet, 720, "ListBullet2", "Symbol")}</w:abstractNum>'
        f'<w:abstractNum w:abstractNumId="3"><w:nsid w:val="5A000003"/><w:multiLevelType w:val="singleLevel"/>{lvl("bullet", bullet, 720, "", "Symbol")}</w:abstractNum>'
        '<w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num>'
        '<w:num w:numId="2"><w:abstractNumId w:val="1"/></w:num>'
        '<w:num w:numId="3"><w:abstractNumId w:val="2"/></w:num>'
        '<w:num w:numId="4"><w:abstractNumId w:val="3"/></w:num><w:num w:numId="5"><w:abstractNumId w:val="3"/></w:num></w:numbering>'
    )


def css(p: Profile) -> str:
    """The PDF side of the same table (selectors from Style.css)."""
    pg = p.page
    r = [
        f"@page {{ size: {pg.width_in}in {pg.height_in}in; margin: {pg.top_in}in {pg.right_in}in {pg.bottom_in}in {pg.left_in}in; }}",
        f"body {{ font-family: '{p.font}'; font-size: {p.size_pt}pt; color: #000; }}",
    ]
    for s in p.styles:
        if not s.css:
            continue
        d: list[str] = []
        if s.font:
            d.append(f"font-family: '{s.font}'")
        if s.size_pt is not None:
            d.append(f"font-size: {s.size_pt}pt")
        if s.bold is not None:
            d.append(f"font-weight: {'bold' if s.bold else 'normal'}")
        if s.italic is not None:
            d.append(f"font-style: {'italic' if s.italic else 'normal'}")
        if s.color:
            d.append(f"color: #{s.color}")
        if s.align:
            d.append(f"text-align: {'justify' if s.align == 'both' else s.align}")
        d.append(f"margin-top: {s.before_pt or 0}pt")
        d.append(f"margin-bottom: {s.after_pt if s.after_pt is not None else p.after_pt}pt")
        if s.left_in:
            d.append(f"margin-left: {s.left_in}in")
        if s.first_line_in:
            d.append(f"text-indent: {s.first_line_in}in")
        if s.line:
            d.append(f"line-height: {s.line[1]}pt" if s.line[0] == "exact" else f"line-height: {s.line[1]}")
        if s.keep_next:
            d.append("break-after: avoid")
        r.append(f"{s.css} {{ {'; '.join(d)}; }}")
    return "\n".join(r)
