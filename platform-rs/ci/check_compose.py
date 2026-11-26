"""Validate compose files against the compose-spec JSON schema (no container engine needed).

    python3 ci/check_compose.py SCHEMA.json FILE.yml [FILE.yml ...]

Exit 0 when every file validates, 1 otherwise. YAML anchors and merge keys are resolved by the YAML loader before validation.
"""

from __future__ import annotations

import json
import sys

import jsonschema
import yaml


def main(argv: list[str]) -> int:
    if len(argv) < 3:
        print(__doc__)
        return 2
    with open(argv[1], encoding="utf-8") as f:
        schema: dict[str, object] = json.load(f)
    bad = 0
    for path in argv[2:]:
        with open(path, encoding="utf-8") as f:
            doc: object = yaml.safe_load(f)
        errors = sorted(jsonschema.Draft7Validator(schema).iter_errors(doc), key=lambda e: list(map(str, e.path)))
        if errors:
            bad += 1
            for e in errors:
                print(f"{path}: {'/'.join(map(str, e.path))}: {e.message[:200]}")
        else:
            print(f"{path}: ok")
    return 1 if bad else 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
