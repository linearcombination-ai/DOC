"""Compare the pipeline's interleaver parts with DOC's oracle parts for the same request.

    python3 -I compare_parts.py OURS.json ORACLE.json

Same number of parts, same order. A part that carries no ULB prose must be byte-identical. The ULB part
differs in markup by design (the model renders `<p class="ulb-text">`-style semantic HTML, DOC's sidecar
path emits `<div class="chapter">`), so it is compared as text: the words of the verses, in order.
"""

from __future__ import annotations

import json
import re
import sys
from html.parser import HTMLParser
from pathlib import Path


class Text(HTMLParser):
    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self.out: list[str] = []
        self.skip = 0

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        cls = dict(attrs).get("class") or ""
        if tag == "span" and ("chaptermarker" in cls or "versemarker" in cls) or (tag in ("p", "sup") and ("ulb-chapter-num" in cls or tag == "sup")):
            self.skip += 1

    def handle_endtag(self, tag: str) -> None:
        if self.skip and tag in ("span", "p", "sup"):
            self.skip -= 1

    def handle_data(self, data: str) -> None:
        if not self.skip:
            self.out.append(data)


def words(html: str) -> list[str]:
    t = Text()
    t.feed(html)
    t.close()
    return re.findall(r"\w+", " ".join(t.out), flags=re.UNICODE)


def main(argv: list[str]) -> int:
    ours = json.loads(Path(argv[0]).read_text())
    oracle = json.loads(Path(argv[1]).read_text())["parts"]
    problems: list[str] = []
    if len(ours) != len(oracle):
        problems.append(f"part count: ours {len(ours)}, DOC {len(oracle)}")
    identical = text_equal = 0
    for i, (a, b) in enumerate(zip(ours, oracle)):
        ca, cb = a["content"], b["content"]
        flags_a = {k: a.get(k) for k in ("add_page_break", "contained_in_two_column_section", "is_rtl", "use_section_visual_separator")}
        flags_b = {k: b.get(k) for k in flags_a}
        if flags_a != flags_b:
            problems.append(f"part {i}: flags differ {flags_a} vs {flags_b}")
        if ca == cb:
            identical += 1
            continue
        wa, wb = words(ca), words(cb)
        if wa == wb:
            text_equal += 1
        else:
            problems.append(f"part {i}: text differs ({len(wa)} vs {len(wb)} words); first difference at word {next((k for k, (x, y) in enumerate(zip(wa, wb)) if x != y), min(len(wa), len(wb)))}")
    print(json.dumps({"parts": len(oracle), "byte_identical": identical, "same_words_different_markup": text_equal, "problems": problems}, indent=1))
    return 0 if not problems else 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
