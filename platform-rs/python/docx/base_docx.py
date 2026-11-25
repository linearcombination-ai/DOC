"""A starting docx written from scratch (no template file, no docxtpl): front matter, TOC
field, two sections, header/footer parts. Only zipfile + string XML."""

import io
import re
import zipfile
from pathlib import Path
from xml.sax.saxutils import escape

from style_table import Profile, numbering_xml, styles_xml, tw

W = 'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"'
REL = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
A = "http://schemas.openxmlformats.org/drawingml/2006/main"
CT = "application/vnd.openxmlformats-officedocument.wordprocessingml"

# (text, bullet numId or 0). "|" splits a bold lead-in from the rest; the two URLs become hyperlinks.
LICENSE: tuple[tuple[str, int], ...] = (
    ("Released under a Creative Commons Attribution-ShareAlike 4.0 International License.", 0),
    ("", 0),
    ("To view a copy of the CC BY-SA 4.0 license visit http://creativecommons.org/licenses/by-sa/4.0/", 0),
    ("", 0),
    ("Below is a human-readable summary of (and not a substitute for) the license.", 0),
    ("You are free to:", 0),
    ("Share|— copy and redistribute the material in any medium or format.", 4),
    ("Adapt|— remix, transform, and build upon the material for any purpose, even commercially.", 4),
    ("The licensor cannot revoke these freedoms as long as you follow the license terms.", 0),
    ("Under the following conditions:", 0),
    ("Attribution|— You must attribute the work as follows: “Original work available at https://BibleInEveryLanguage.org.” Attribution statements in derivative works should not in any way suggest that we endorse you or your use of this work.", 5),
    ("ShareAlike|— If you remix, transform, or build upon the material, you must distribute your contributions under the same license as the original.", 5),
    ("No additional restrictions|— You may not apply legal terms or technological measures that legally restrict others from doing anything the license permits.", 5),
    ("Notices:", 0),
    ("You do not have to comply with the license for elements of the material in the public domain or where your use is permitted by an applicable exception or limitation.", 0),
    ("", 0),
    ("No warranties are given. The license may not give you all of the permissions necessary for your intended use. For example, other rights such as publicity, privacy, or moral rights may limit how you use the material.", 0),
    ("", 0),
)
LINKS = {"http://creativecommons.org/licenses/by-sa/4.0/": "rIdL1", "https://BibleInEveryLanguage.org": "rIdL2"}
MEDIA = Path(__file__).parent / "media"  # image1.png (CC badge), image2.png (logo): binary assets shipped next to the code


def runs_for(text: str, rpr: str = "") -> str:
    """Runs for text, turning the known URLs into hyperlinks (style PlainHyperlinkChar0)."""
    out = ""
    for piece in re.split("(" + "|".join(re.escape(u) for u in LINKS) + ")", text):
        if not piece:
            continue
        if piece in LINKS:
            out += f'<w:hyperlink r:id="{LINKS[piece]}" w:history="1"><w:r><w:rPr><w:rStyle w:val="PlainHyperlinkChar0"/>{rpr}</w:rPr><w:t>{escape(piece)}</w:t></w:r></w:hyperlink>'
        else:
            r = f"<w:rPr>{rpr}</w:rPr>" if rpr else ""
            out += f'<w:r>{r}<w:t xml:space="preserve">{escape(piece)}</w:t></w:r>'
    return out


def inline_picture(rid: str, cx: int, cy: int, name: str) -> str:
    return (
        f'<w:r><w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="{cx}" cy="{cy}"/>'
        f'<wp:docPr id="1" name="{name}" descr="Creative Commons BY-SA badge"/><a:graphic xmlns:a="{A}"><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture">'
        f'<pic:pic xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:nvPicPr><pic:cNvPr id="1" name="{name}"/><pic:cNvPicPr/></pic:nvPicPr>'
        f'<pic:blipFill><a:blip r:embed="{rid}"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>'
        f'<pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr>'
        "</pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>"
    )


def anchored_logo(rid: str, docpr_id: int = 2) -> str:
    cx, cy = 1100667, 266223
    return (
        '<w:r><w:drawing><wp:anchor distT="0" distB="0" distL="114300" distR="114300" simplePos="0" relativeHeight="251658240" '
        'behindDoc="1" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/>'
        '<wp:positionH relativeFrom="column"><wp:posOffset>-29633</wp:posOffset></wp:positionH>'
        '<wp:positionV relativeFrom="paragraph"><wp:posOffset>28575</wp:posOffset></wp:positionV>'
        f'<wp:extent cx="{cx}" cy="{cy}"/><wp:effectExtent l="0" t="0" r="4445" b="635"/><wp:wrapNone/>'
        f'<wp:docPr id="{docpr_id}" name="Logo"/><wp:cNvGraphicFramePr/>'
        f'<a:graphic xmlns:a="{A}"><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture">'
        f'<pic:nvPicPr><pic:cNvPr id="{docpr_id}" name="Logo"/><pic:cNvPicPr/></pic:nvPicPr>'
        f'<pic:blipFill><a:blip r:embed="{rid}"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>'
        f'<pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr>'
        "</pic:pic></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"
    )


def para(style: str, text: str = "", extra_ppr: str = "", runs: str | None = None) -> str:
    ppr = f'<w:pPr><w:pStyle w:val="{style}"/>{extra_ppr}</w:pPr>'
    body = runs if runs is not None else (f'<w:r><w:t xml:space="preserve">{escape(text)}</w:t></w:r>' if text else "")
    return f"<w:p>{ppr}{body}</w:p>"


def field(instr: str, cached: str, rpr: str = "") -> str:
    r = f"<w:rPr>{rpr}</w:rPr>" if rpr else ""
    return (
        f'<w:r>{r}<w:fldChar w:fldCharType="begin"/></w:r>'
        f'<w:r>{r}<w:instrText xml:space="preserve"> {instr} </w:instrText></w:r>'
        f'<w:r>{r}<w:fldChar w:fldCharType="separate"/></w:r>'
        f'<w:r>{r}<w:t>{escape(cached)}</w:t></w:r>'
        f'<w:r>{r}<w:fldChar w:fldCharType="end"/></w:r>'
    )


def page_break() -> str:
    return '<w:p><w:r><w:br w:type="page"/></w:r></w:p>'


def sect(profile: Profile, cover: bool, header: str, footer: str) -> str:
    pg = profile.page
    w, h = (11907, 16839) if cover else (tw(pg.width_in), tw(pg.height_in))
    return (
        f'<w:sectPr><w:headerReference w:type="default" r:id="{header}"/><w:footerReference w:type="default" r:id="{footer}"/>'
        f'<w:pgSz w:w="{w}" w:h="{h}"/><w:pgMar w:top="{tw(pg.top_in)}" w:right="{tw(pg.right_in)}" '
        f'w:bottom="{tw(pg.bottom_in)}" w:left="{tw(pg.left_in)}" w:header="{tw(pg.header_in)}" '
        f'w:footer="{tw(pg.footer_in)}" w:gutter="0"/><w:cols w:space="720"/></w:sectPr>'
    )


def document_xml(profile: Profile, title1: str, title2: str, title3: str) -> str:
    body = [
        para("Title", title1),
        para("Subtitle", title2),
        para("Subtitle", title3),
        para("Subtitle"),
        para("Subtitle"),
        para("Normal"),
        para("Normal"),
        para("Normal"),
        f'<w:p><w:pPr><w:pStyle w:val="LicenseText"/></w:pPr><w:r><w:br w:type="page"/></w:r>{runs_for("©2026 Wycliffe Associates")}</w:p>',
    ]
    for text, bullet in LICENSE:
        num = f'<w:numPr><w:ilvl w:val="0"/><w:numId w:val="{bullet}"/></w:numPr>' if bullet else ""
        if "|" in text:
            lead, rest = text.split("|", 1)
            runs = f"<w:r><w:rPr><w:b/><w:bCs/></w:rPr><w:t>{escape(lead)}</w:t></w:r>" + runs_for(rest)
        else:
            runs = runs_for(text)
        body.append(para("LicenseText", extra_ppr=num, runs=runs))
    body += [
        para("LicenseText", runs=inline_picture("rIdI1", 658368, 228600, "CC BY-SA")),
        para("Normal"),
        page_break(),
        para("BodyText", runs=field(r'TOC \o "1-2" \h \z \u', "Right-click to update field (doing so will insert table of contents).")),
        para("BodyText"),
        page_break(),
        para("BodyText", "Page left intentionally blank"),
        f'<w:p><w:pPr><w:pStyle w:val="BodyText"/>{sect(profile, True, "rIdH1", "rIdF1")}</w:pPr></w:p>',
        para("PlainHyperlink", " "),
        para("Normal"),
    ]
    return f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document {W}><w:body>{"".join(body)}{sect(profile, False, "rIdH2", "rIdF2")}</w:body></w:document>'


def hdr(tag: str, content: str) -> str:
    return f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:{tag} {W}>{content}</w:{tag}>'


def build_base(profile: Profile, title1: str, title2: str, title3: str = "", book: str = "") -> bytes:
    # P3b: the running header of the sample, "Book [tab] first resource on the page [tab] chapter", as fields:
    # STYLEREF "heading 1" is the first Heading 1 on the page (the resource title); the chapter is the last
    # drop-cap chapter number paragraph (style "ULB Chapter Number") at or before the page.
    tab = "<w:r><w:tab/></w:r>"
    running = para(
        "Header",
        extra_ppr="",
        runs=f'<w:r><w:t xml:space="preserve">{escape(book)}</w:t></w:r>{tab}'
        + field('STYLEREF  "heading 1"  \\* MERGEFORMAT', "Translation Notes")
        + tab
        + field('STYLEREF  "ULB Chapter Number"  \\* MERGEFORMAT', "1"),
    )
    page_x_of_y = para(
        "Footer", extra_ppr='<w:jc w:val="center"/>',
        runs=anchored_logo("rIdI2") + '<w:r><w:t xml:space="preserve">Page </w:t></w:r>' + field("PAGE  \\* Arabic  \\* MERGEFORMAT", "1", "<w:b/><w:bCs/>")
        + '<w:r><w:t xml:space="preserve"> of </w:t></w:r>' + field("NUMPAGES  \\* Arabic  \\* MERGEFORMAT", "1", "<w:b/><w:bCs/>"),
    )
    parts = {
        "[Content_Types].xml": (
            '<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
            '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
            '<Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/>'
            f'<Override PartName="/word/document.xml" ContentType="{CT}.document.main+xml"/>'
            f'<Override PartName="/word/styles.xml" ContentType="{CT}.styles+xml"/>'
            f'<Override PartName="/word/numbering.xml" ContentType="{CT}.numbering+xml"/>'
            f'<Override PartName="/word/settings.xml" ContentType="{CT}.settings+xml"/>'
            f'<Override PartName="/word/header1.xml" ContentType="{CT}.header+xml"/>'
            f'<Override PartName="/word/header2.xml" ContentType="{CT}.header+xml"/>'
            f'<Override PartName="/word/footer1.xml" ContentType="{CT}.footer+xml"/>'
            f'<Override PartName="/word/footer2.xml" ContentType="{CT}.footer+xml"/>'
            '<Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/></Types>'
        ),
        "_rels/.rels": (
            '<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
            f'<Relationship Id="rId1" Type="{REL}/officeDocument" Target="word/document.xml"/>'
            '<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/></Relationships>'
        ),
        "docProps/core.xml": (
            '<?xml version="1.0" encoding="UTF-8" standalone="yes"?><cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" '
            'xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>DOC</dc:title></cp:coreProperties>'
        ),
        "word/_rels/document.xml.rels": (
            '<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
            f'<Relationship Id="rIdS" Type="{REL}/styles" Target="styles.xml"/>'
            f'<Relationship Id="rIdN" Type="{REL}/numbering" Target="numbering.xml"/>'
            f'<Relationship Id="rIdT" Type="{REL}/settings" Target="settings.xml"/>'
            f'<Relationship Id="rIdH1" Type="{REL}/header" Target="header1.xml"/>'
            f'<Relationship Id="rIdH2" Type="{REL}/header" Target="header2.xml"/>'
            f'<Relationship Id="rIdF1" Type="{REL}/footer" Target="footer1.xml"/>'
            f'<Relationship Id="rIdF2" Type="{REL}/footer" Target="footer2.xml"/>'
            f'<Relationship Id="rIdL1" Type="{REL}/hyperlink" Target="http://creativecommons.org/licenses/by-sa/4.0/" TargetMode="External"/>'
            f'<Relationship Id="rIdL2" Type="{REL}/hyperlink" Target="https://BibleInEveryLanguage.org" TargetMode="External"/>'
            f'<Relationship Id="rIdI1" Type="{REL}/image" Target="media/image1.png"/></Relationships>'
        ),
        "word/document.xml": document_xml(profile, title1, title2, title3),
        "word/styles.xml": styles_xml(profile),
        "word/numbering.xml": numbering_xml(),
        "word/settings.xml": f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:settings {W}><w:defaultTabStop w:val="720"/></w:settings>',
        "word/header1.xml": hdr("hdr", para("Header")),
        "word/header2.xml": hdr("hdr", running),
        "word/footer1.xml": hdr("ftr", para("Footer", extra_ppr='<w:jc w:val="center"/>', runs=anchored_logo("rIdI3", 3))),
        "word/_rels/footer1.xml.rels": (
            '<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
            f'<Relationship Id="rIdI3" Type="{REL}/image" Target="media/image2.png"/></Relationships>'
        ),
        "word/footer2.xml": hdr("ftr", page_x_of_y),
        "word/_rels/footer2.xml.rels": (
            '<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
            f'<Relationship Id="rIdI2" Type="{REL}/image" Target="media/image2.png"/></Relationships>'
        ),
    }
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w", zipfile.ZIP_DEFLATED) as z:
        for name, xml in parts.items():
            z.writestr(name, xml)
        z.write(MEDIA / "image1.png", "word/media/image1.png")
        z.write(MEDIA / "image2.png", "word/media/image2.png")
    return buf.getvalue()
