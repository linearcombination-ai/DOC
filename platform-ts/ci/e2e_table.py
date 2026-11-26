"""Pass table of DOC's own end-to-end tests (tests/e2e/doc) on this build's stack against DOC's FastAPI baseline, from junit XML files.

    python ci/e2e_table.py --ts <dir with junit files> --doc <dir with junit files> [--out table.md]

Each directory holds one or more junit XML files (the CI runs three groups per stack; a directory is searched recursively for `*.xml`). The output is
markdown: one row per test file (passed / failed / skipped on each stack), then every test whose outcome differs, with the first line of the
failure message, then the failures both stacks share. mypy --strict clean, no typing.Any.
"""

from __future__ import annotations

import argparse
import re
import sys
import xml.etree.ElementTree as ET
from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True)
class Outcome:
    status: str  # passed | failed | skipped
    detail: str


def module_of(classname: str) -> str:
    """`tests.e2e.doc.test_api.TestX` or `tests.e2e.doc.test_api` -> `test_api.py`; a Playwright class name containing `doc_test_3.ts` -> `doc_test_3.ts`."""
    playwright = re.search(r"[A-Za-z_]+_test(?:_\d+)?\.ts", classname)
    if playwright:
        return playwright.group(0)
    parts = classname.split(".")
    for i, part in enumerate(parts):
        if part.startswith("test_"):
            return f"{part}.py"
        if i == len(parts) - 1:
            return f"{part}.py"
    return classname


def first_line(text: str) -> str:
    for line in text.strip().splitlines():
        if line.strip():
            return line.strip()[:200]
    return ""


def load(directory: Path) -> dict[str, Outcome]:
    out: dict[str, Outcome] = {}
    for path in sorted(directory.rglob("*.xml")):
        try:
            root = ET.parse(path).getroot()
        except ET.ParseError:
            continue
        for case in root.iter("testcase"):
            name = f"{module_of(case.get('classname', ''))}::{case.get('name', '')}"
            failure = case.find("failure")
            error = case.find("error")
            skipped = case.find("skipped")
            if failure is not None:
                out[name] = Outcome("failed", first_line(failure.get("message", "") or (failure.text or "")))
            elif error is not None:
                out[name] = Outcome("failed", first_line(error.get("message", "") or (error.text or "")))
            elif skipped is not None:
                out[name] = Outcome("skipped", first_line(skipped.get("message", "")))
            else:
                out[name] = Outcome("passed", "")
    return out


def counts(results: dict[str, Outcome], module: str) -> tuple[int, int, int]:
    rows = [o for n, o in results.items() if n.startswith(module + "::")]
    return (sum(o.status == "passed" for o in rows), sum(o.status == "failed" for o in rows), sum(o.status == "skipped" for o in rows))


def cell(o: Outcome | None) -> str:
    return o.detail.replace("|", "/") if o else ""


def render(ts: dict[str, Outcome], doc: dict[str, Outcome]) -> str:
    modules = sorted({n.split("::")[0] for n in [*ts, *doc]})
    lines = [
        "| test file | tests | TS pass | TS fail | TS skip | DOC pass | DOC fail | DOC skip |",
        "|---|---:|---:|---:|---:|---:|---:|---:|",
    ]
    totals = [0, 0, 0, 0, 0, 0, 0]
    for m in modules:
        tp, tf, tsk = counts(ts, m)
        dp, df, dsk = counts(doc, m)
        total = max(tp + tf + tsk, dp + df + dsk)
        lines.append(f"| {m} | {total} | {tp} | {tf} | {tsk} | {dp} | {df} | {dsk} |")
        for i, v in enumerate([total, tp, tf, tsk, dp, df, dsk]):
            totals[i] += v
    lines.append("| **total** | " + " | ".join(f"**{t}**" for t in totals) + " |")
    differ = [(n, ts.get(n), doc.get(n)) for n in sorted({*ts, *doc}) if (ts.get(n) or Outcome("absent", "")).status != (doc.get(n) or Outcome("absent", "")).status]
    lines += ["", f"### Outcome differs between the stacks ({len(differ)})", "", "| test | TS | DOC | TS detail | DOC detail |", "|---|---|---|---|---|"]
    for name, a, b in differ:
        lines.append(f"| {name} | {a.status if a else 'absent'} | {b.status if b else 'absent'} | {cell(a)} | {cell(b)} |")
    both = [(n, ts[n]) for n in sorted(ts) if ts[n].status == "failed" and n in doc and doc[n].status == "failed"]
    lines += ["", f"### Fail on both stacks ({len(both)})", ""]
    lines += [f"* `{n}`: {o.detail}" for n, o in both]
    return "\n".join(lines) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--ts", type=Path, required=True)
    parser.add_argument("--doc", type=Path, required=True)
    parser.add_argument("--out", type=Path)
    args = parser.parse_args()
    text = render(load(args.ts), load(args.doc))
    if args.out:
        args.out.write_text(text, encoding="utf-8")
    sys.stdout.write(text)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
