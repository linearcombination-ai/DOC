"""Check that every `build:` entry of a compose file points at a directory and a Dockerfile that exist (the images are built in CI by file path,
so a wrong `context` or `dockerfile` in the compose file, which `docker compose up --build` uses on the captain's machine, would only show up there).

    python3 ci/check_build_paths.py docker-compose.yml

Paths are resolved the way compose resolves them: `context` against the directory of the compose file, `dockerfile` against the context. No container engine needed.
"""

from __future__ import annotations

import sys
from pathlib import Path

import yaml


def main(argv: list[str]) -> int:
    bad = 0
    for name in argv[1:]:
        path = Path(name).resolve()
        with open(path, encoding="utf-8") as f:
            doc: object = yaml.safe_load(f)
        services = doc.get("services", {}) if isinstance(doc, dict) else {}
        for service, spec in services.items():
            build = spec.get("build") if isinstance(spec, dict) else None
            if build is None:
                continue
            context_text = build if isinstance(build, str) else str(build.get("context", "."))
            dockerfile_text = "Dockerfile" if isinstance(build, str) else str(build.get("dockerfile", "Dockerfile"))
            context = (path.parent / context_text).resolve()
            dockerfile = context / dockerfile_text
            ok = context.is_dir() and dockerfile.is_file()
            print(f"{'ok  ' if ok else 'FAIL'} {service}: context {context} dockerfile {dockerfile}")
            bad += 0 if ok else 1
    return 1 if bad else 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
