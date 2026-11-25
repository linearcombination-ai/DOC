"""Check a docx the way a validator would at the package level: a whole zip, every XML part well-formed,
the required parts present, paragraphs in the body. Prints one JSON line; exit 0 when valid."""

from __future__ import annotations

import json
import sys
import zipfile
from xml.etree import ElementTree

REQUIRED = ("[Content_Types].xml", "_rels/.rels", "word/document.xml", "word/styles.xml")
W = "{http://schemas.openxmlformats.org/wordprocessingml/2006/main}"


def check(path: str) -> dict[str, object]:
    with zipfile.ZipFile(path) as z:
        bad = z.testzip()
        if bad is not None:
            raise ValueError(f"corrupt member {bad}")
        names = z.namelist()
        for r in REQUIRED:
            if r not in names:
                raise ValueError(f"missing part {r}")
        for n in names:
            if n.endswith((".xml", ".rels")):
                ElementTree.fromstring(z.read(n))
        body = ElementTree.fromstring(z.read("word/document.xml"))
        paragraphs = body.findall(f".//{W}p")
        headings = [p for p in paragraphs if p.find(f"{W}pPr/{W}pStyle") is not None and p.find(f"{W}pPr/{W}pStyle").get(f"{W}val", "").startswith("Heading")]  # type: ignore[union-attr]
        text = "".join(t.text or "" for t in body.iter(f"{W}t"))
        return {"ok": True, "parts": len(names), "paragraphs": len(paragraphs), "headings": len(headings), "chars": len(text), "has_jude": "Jude" in text}


if __name__ == "__main__":
    try:
        print(json.dumps(check(sys.argv[1])))
    except (OSError, ValueError, zipfile.BadZipFile, ElementTree.ParseError, KeyError) as e:
        print(json.dumps({"ok": False, "error": f"{type(e).__name__}: {e}"}))
        sys.exit(1)
