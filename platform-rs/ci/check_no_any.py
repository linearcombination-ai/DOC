"""Fail when Python code in the given directories imports or uses typing.Any (a standing rule of this project). AST based, so docstrings and comments that mention it are fine."""

from __future__ import annotations

import ast
import sys
from pathlib import Path


def violations(path: Path) -> list[str]:
    found: list[str] = []
    tree = ast.parse(path.read_text(encoding="utf-8"), filename=str(path))
    for node in ast.walk(tree):
        if isinstance(node, ast.ImportFrom) and node.module == "typing" and any(a.name == "Any" for a in node.names):
            found.append(f"{path}:{node.lineno}: from typing import Any")
        elif isinstance(node, ast.Attribute) and node.attr == "Any" and isinstance(node.value, ast.Name) and node.value.id in ("typing", "t"):
            found.append(f"{path}:{node.lineno}: typing.Any")
    return found


def main(argv: list[str]) -> int:
    bad: list[str] = []
    for root in argv:
        for path in sorted(Path(root).rglob("*.py")):
            if "node_modules" in path.parts or "vendor" in path.parts:
                continue
            bad.extend(violations(path))
    for line in bad:
        print(line)
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
