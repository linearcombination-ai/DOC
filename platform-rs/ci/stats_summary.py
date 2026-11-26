"""Summarize `docker stats` samples (ci/stats-sampler.sh CSV files) into one table: per container, the peak and median memory (MiB) and the peak CPU.

    python ci/stats_summary.py <csv> [<csv> ...]        markdown table on stdout; several files (groups of one run) are pooled

Container names are reduced to the service name (`doc-rs-worker-1` -> `worker`, `doc-api-1` -> `api`). mypy --strict clean, no typing.Any.
"""

from __future__ import annotations

import csv
import re
import statistics
import sys
from pathlib import Path

UNITS = {"B": 1 / 1048576, "KiB": 1 / 1024, "MiB": 1.0, "GiB": 1024.0, "kB": 1 / 1024, "MB": 1.0, "GB": 1024.0}


def mib(text: str) -> float | None:
    """`302.5MiB / 3GiB` -> 302.5 (the used part only)."""
    m = re.match(r"\s*([\d.]+)\s*([A-Za-z]+)", text.split("/")[0])
    if not m or m.group(2) not in UNITS:
        return None
    return float(m.group(1)) * UNITS[m.group(2)]


def service(container: str) -> str:
    m = re.match(r"^(?:doc-rs|platform-rs|doc)-(.+?)-\d+$", container)
    return m.group(1) if m else container


def main(paths: list[str]) -> int:
    memory: dict[str, list[float]] = {}
    cpu: dict[str, list[float]] = {}
    for path in paths:
        with open(path, newline="", encoding="utf-8") as f:
            for row in csv.DictReader(f):
                name = service(row.get("container", ""))
                used = mib(row.get("mem_usage", ""))
                if used is not None:
                    memory.setdefault(name, []).append(used)
                try:
                    cpu.setdefault(name, []).append(float(row.get("cpu_percent", "0").rstrip("%")))
                except ValueError:
                    pass
    print("| container | samples | memory median (MiB) | memory peak (MiB) | CPU peak (%) |")
    print("|---|---:|---:|---:|---:|")
    for name in sorted(memory):
        m, c = memory[name], cpu.get(name, [0.0])
        print(f"| {name} | {len(m)} | {statistics.median(m):.0f} | {max(m):.0f} | {max(c):.0f} |")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
