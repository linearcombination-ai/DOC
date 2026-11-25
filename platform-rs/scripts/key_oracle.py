"""Differential oracle for the document request key: DOC's own `backend/doc/utils/request_key.py` (imported by
path, stdlib only) plus a transcription of `document_request_key` / `select_assembly_layout_kind` from
`document_generator.py` (pydantic is not needed for hashing). The transcription is validated against the
three golden keys DOC pins (two in tests/unit/test_document_generator.py, one from the interleave oracle)
before it generates anything.

    python3 -I key_oracle.py DOC_ROOT OUT.json [N]
"""

from __future__ import annotations

import importlib.util
import json
import random
import re
import sys
from collections.abc import Mapping
from pathlib import Path
from types import ModuleType

DEFAULTS: dict[str, object] = {
    "assembly_layout_kind": "1c", "layout_for_print": False, "generate_docx": False, "limit_words": True,
    "use_chapter_labels": False, "use_prince": False, "use_two_column_layout_for_tn_notes": False,
    "use_two_column_layout_for_tq_notes": False, "use_section_visual_separator": False,
    "show_tn_book_intro": True, "show_bc_book_intro": True, "show_tn_chapter_intro": True, "show_bc_chapter_commentary": True,
}
STRATEGIES = ["lbo", "blo", "lvo", "bvo", "stet"]
RESOURCE_TYPES = ["ulb", "udb", "reg", "tn", "tn-condensed", "tq", "tw", "bc", "cuv", "nav", "blv", "f10"]
BOOKS = ["gen", "exo", "mat", "mrk", "luk", "jhn", "act", "rom", "jud", "3jn", "tit", "phm", "rev", "1co", "psa", "isa"]
LANGS = ["en", "es-419", "fr", "sw", "pt-br", "hi", "km", "ur", "arb", "ru", "zh", "tpi", "ach-SS-acholi"]


def load(doc_root: Path) -> ModuleType:
    spec = importlib.util.spec_from_file_location("request_key", doc_root / "backend/doc/utils/request_key.py")
    assert spec is not None and spec.loader is not None
    mod = importlib.util.module_from_spec(spec)
    sys.modules["request_key"] = mod
    spec.loader.exec_module(mod)
    return mod


def resolve_layout(req: Mapping[str, object]) -> str:
    source = str(req.get("document_request_source", "test"))
    layout = req.get("assembly_layout_kind", "1c")
    strategy = req["assembly_strategy_kind"]
    if source == "test" and layout:
        return str(layout)
    if strategy == "stet":
        return "stet"
    return "1c_c" if req.get("layout_for_print", False) else "1c"


def key_of(rk: ModuleType, req: Mapping[str, object], resolve: bool) -> str:
    """`resolve=False` is what the unit tests' golden calls do (document_request_key on a model that never went
    through initialize_document_request_and_key); `resolve=True` is the real path."""
    resources = req["resource_requests"]
    assert isinstance(resources, list)
    settings = {f: req.get(f, d) for f, d in DEFAULTS.items()}
    settings["assembly_strategy_kind"] = req["assembly_strategy_kind"]
    if resolve:
        settings["assembly_layout_kind"] = resolve_layout(req)
    elif "assembly_layout_kind" in req:
        settings["assembly_layout_kind"] = req["assembly_layout_kind"]
    if not any(re.match("tw.*", str(r["resource_type"])) for r in resources):
        settings["limit_words"] = None
    slug = rk.resources_slug([r["lang_code"] for r in resources], [r["resource_type"] for r in resources], [r["book_code"] for r in resources])
    return str(rk.request_key(slug, {"resources": [[r["lang_code"], r["resource_type"], r["book_code"]] for r in resources], "settings": {k: settings[k] for k in sorted(settings)}}))


def rr(lang: str, rt: str, book: str) -> dict[str, str]:
    return {"lang_code": lang, "resource_type": rt, "book_code": book}


def main(argv: list[str]) -> int:
    doc_root, out = Path(argv[0]), Path(argv[1])
    n = int(argv[2]) if len(argv) > 2 else 300
    rk = load(doc_root)
    # the three goldens
    g1 = {"assembly_strategy_kind": "lbo", "resource_requests": [rr("en", "ulb", "mat")]}
    assert key_of(rk, g1, resolve=False) == "en_ulb_mat_70c7420bb621", key_of(rk, g1, False)
    g2 = {"assembly_strategy_kind": "lbo", "assembly_layout_kind": "1c", "document_request_source": "ui", "limit_words": False, "resource_requests": [rr("en", "ulb", "mat"), rr("en", "tw", "mat")]}
    assert key_of(rk, g2, resolve=False) == "en_ulb-tw_mat_50944a0aa352", key_of(rk, g2, False)
    g3 = {"assembly_strategy_kind": "lbo", "document_request_source": "ui", "resource_requests": [rr("en", "ulb", "jud"), rr("en", "tn", "jud")]}
    assert key_of(rk, g3, resolve=True) == "en_ulb-tn_jud_d404fded3466", key_of(rk, g3, True)
    cases: list[dict[str, object]] = [
        {"name": "golden_test_default", "request": g1, "key": key_of(rk, g1, False)},
        {"name": "golden_ui_tw", "request": g2, "key": key_of(rk, g2, False)},
        {"name": "oracle_jud_ulb_tn", "request": g3, "key": key_of(rk, g3, True)},
    ]
    rnd = random.Random(20261125)
    for i in range(n):
        k = rnd.randint(1, 5)
        req: dict[str, object] = {
            "assembly_strategy_kind": rnd.choice(STRATEGIES),
            "resource_requests": [rr(rnd.choice(LANGS), rnd.choice(RESOURCE_TYPES), rnd.choice(BOOKS)) for _ in range(k)],
            "document_request_source": rnd.choice(["ui", "test", "biel_ui"]),
        }
        for f, d in DEFAULTS.items():
            if rnd.random() < 0.3:
                req[f] = (rnd.choice(["1c", "1c_c", "2c_sl_sr", "2c_sl_sr_c", "stet", None]) if f == "assembly_layout_kind" else (not d))
        for f in ("email_address", "generate_pdf", "generate_epub"):
            if rnd.random() < 0.3:
                req[f] = {"email_address": "a@b.co", "generate_pdf": rnd.random() < 0.5, "generate_epub": False}[f]
        cases.append({"name": f"random_{i:03d}", "request": req, "key": key_of(rk, req, resolve=True)})
    slice_docx = dict(g3, generate_docx=True, generate_pdf=False)
    out.write_text(json.dumps({"doc_commit": "50de21cd3bab5c6f48961da64257118f6f0ead0e", "slice_docx_key": key_of(rk, slice_docx, resolve=True), "cases": cases}, indent=1))
    print(f"{len(cases)} cases written, goldens reproduced")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
