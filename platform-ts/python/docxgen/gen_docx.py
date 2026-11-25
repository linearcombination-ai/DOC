"""Template-free docx from the core's proposed-look parts (P3b).

    python gen_docx.py PARTS.json OUT.docx [--book Matthew] [--title1 ...] [--title2 ...]

The base document (styles, numbering, front matter, header and footer parts) is written from the style
table in `profile_look.py` by `base_docx.build_base`, with no template file and no docxtpl; the body is built
here from the semantic classes the core emits (`h1.resource-title`, `p.indented`, `p.ulb-text`, ...), mapped
to styles by one table (`STYLE_FOR`). The same table drives the PDF CSS (`style_table.css`).
What it does not do is listed in the report (hyperlinks are plain text, no section breaks, no highlights).
mypy --strict clean, no typing.Any.
"""

from __future__ import annotations

import argparse
import io
import json
import re
import sys
import zipfile
from html.parser import HTMLParser
from pathlib import Path
from xml.sax.saxutils import escape

sys.path.insert(0, str(Path(__file__).parent))
import base_docx  # noqa: E402
from profile_look import LOOK  # noqa: E402
from style_table import styles_xml  # noqa: E402

# (tag, class) -> style id. One table; the CSS selectors in profile_look.py are the same keys.
STYLE_FOR: dict[tuple[str, str], str] = {
    ("h1", "resource-title"): "Heading1",
    ("h2", ""): "Heading2", ("h2", "tn-verse-ref"): "Heading2",
    ("h3", ""): "Heading3",
    ("h4", ""): "Heading4", ("h4", "tn-note"): "Heading4", ("h4", "bc-question"): "Heading4",
    ("h5", ""): "Heading5",
    ("p", ""): "Normal", ("p", "indented"): "IndentedDefinitionText", ("p", "tw-list"): "Normal", ("p", "line-space"): "LineSpace",
    ("p", "ulb-text"): "UlbText", ("p", "ulb-poetry-1"): "UlbPoetry1", ("p", "ulb-poetry-2"): "UlbPoetry2",
    ("p", "ulb-chapter-num"): "UlbChapterNumber",
    ("li", "num-2"): "ListNumber2", ("li", "num-3"): "ListNumber3", ("li", "bullet-3"): "ListBullet3",
}
BLOCKS = {"h1", "h2", "h3", "h4", "h5", "h6", "p", "li"}
FRAME = '<w:framePr w:dropCap="drop" w:lines="2" w:hSpace="72" w:wrap="around" w:vAnchor="text" w:hAnchor="text"/>'


class Body(HTMLParser):
    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self.xml: list[str] = []
        self.cur: tuple[str, str] | None = None
        self.runs: list[str] = []
        self.sup = False
        self.list_num_id = 0
        self.next_num_id = 20
        self.nums: list[int] = []
        self.list_kind = ""
        self.warnings: list[str] = []

    # numbering: every <ol> gets its own w:num so numbering restarts at 1
    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        cls = next((v or "" for k, v in attrs if k == "class"), "")
        if tag in ("ol", "ul"):
            self.list_kind = tag
            if tag == "ol":
                self.list_num_id = self.next_num_id
                self.nums.append(self.list_num_id)
                self.next_num_id += 1
        elif tag in BLOCKS:
            self.cur = (tag, cls.split()[0] if cls else "")
            self.runs = []
        elif tag == "sup":
            self.sup = True
        elif tag == "br":
            self.runs.append("<w:r><w:br/></w:r>")

    def handle_endtag(self, tag: str) -> None:
        if tag == "sup":
            self.sup = False
        elif tag in ("ol", "ul"):
            self.list_kind = ""
        elif tag in BLOCKS and self.cur and self.cur[0] == tag:
            self._emit()
            self.cur = None

    def handle_data(self, data: str) -> None:
        if self.cur is None:
            return
        for i, piece in enumerate(re.split(r"(\t)", data)):
            if piece == "\t":
                self.runs.append("<w:r><w:tab/></w:r>")
            elif piece:
                text = piece.replace("\n", " ")
                if not text.strip() and not self.runs:
                    continue
                rpr = '<w:rPr><w:rStyle w:val="VerseNumber"/></w:rPr>' if self.sup else ""
                self.runs.append(f'<w:r>{rpr}<w:t xml:space="preserve">{escape(text)}</w:t></w:r>')
            _ = i

    def _emit(self) -> None:
        assert self.cur
        tag, cls = self.cur
        style = STYLE_FOR.get((tag, cls)) or STYLE_FOR.get((tag, ""))
        if style is None:
            self.warnings.append(f"no style for <{tag} class={cls!r}>")
            style = "Normal"
        ppr = f'<w:pStyle w:val="{style}"/>'
        if style == "UlbChapterNumber":
            ppr += f'<w:keepNext/>{FRAME}'
        if tag == "li":
            num = self.list_num_id if self.list_kind == "ol" else (3 if cls == "bullet-3" else 3)
            ppr += f'<w:numPr><w:ilvl w:val="0"/><w:numId w:val="{num}"/></w:numPr>'
        runs = "".join(self.runs)
        if style == "LineSpace":
            runs = ""
        if style == "Heading1" or runs.strip() or style in ("LineSpace",):
            self.xml.append(f"<w:p><w:pPr>{ppr}</w:pPr>{runs}</w:p>")


def numbering_patch(xml: str, nums: list[int]) -> str:
    """Add abstractNums for List Number 2/3 (and a bullet at level 3) and one w:num per ordered list."""
    def lvl(fmt: str, text: str, left: int, font: str = "") -> str:
        rpr = f'<w:rPr><w:rFonts w:ascii="{font}" w:hAnsi="{font}" w:hint="default"/></w:rPr>' if font else ""
        return (f'<w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="{fmt}"/><w:lvlText w:val="{text}"/>'
                f'<w:lvlJc w:val="left"/><w:pPr><w:ind w:left="{left}" w:hanging="360"/></w:pPr>{rpr}</w:lvl>')
    abstract = (
        f'<w:abstractNum w:abstractNumId="10"><w:nsid w:val="5A00000A"/><w:multiLevelType w:val="singleLevel"/>{lvl("decimal", "%1.", 720)}</w:abstractNum>'
    )
    nums_xml = "".join(f'<w:num w:numId="{n}"><w:abstractNumId w:val="10"/></w:num>' for n in nums)
    first_num = xml.index("<w:num ")
    return xml[:first_num] + abstract + xml[first_num:].replace("</w:numbering>", nums_xml + "</w:numbering>")


def build(parts: list[dict[str, object]], book: str, title1: str, title2: str) -> tuple[bytes, list[str]]:
    base = base_docx.build_base(LOOK, title1, title2, "", book)
    p = Body()
    for part in parts:
        c = part.get("content")
        if isinstance(c, str):
            p.feed(c)
    p.close()
    zin = zipfile.ZipFile(io.BytesIO(base))
    out = io.BytesIO()
    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
        for info in zin.infolist():
            data = zin.read(info.filename)
            if info.filename == "word/document.xml":
                s = data.decode("utf-8")
                cut = s.rindex("<w:sectPr>")
                s = s[:cut] + "".join(p.xml) + s[cut:]
                data = s.encode("utf-8")
            elif info.filename == "word/styles.xml":
                data = styles_xml(LOOK).encode("utf-8")
            elif info.filename == "word/numbering.xml":
                data = numbering_patch(data.decode("utf-8"), p.nums).encode("utf-8")
            z.writestr(info.filename, data)
    return out.getvalue(), p.warnings


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("parts")
    ap.add_argument("out")
    ap.add_argument("--book", default="Matthew")
    ap.add_argument("--title1", default="English (English): Bible Commentary, Translation Notes, Translation Words, Unlocked Literal Bible")
    ap.add_argument("--title2", default="Matthew 1-2")
    a = ap.parse_args(argv)
    parts = json.loads(Path(a.parts).read_text(encoding="utf-8"))
    data, warnings = build(parts, a.book, a.title1, a.title2)
    Path(a.out).write_bytes(data)
    for w in sorted(set(warnings)):
        print("warning:", w, file=sys.stderr)
    print(f"wrote {a.out} ({len(data)} bytes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
