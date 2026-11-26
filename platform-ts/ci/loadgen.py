"""HTTP load generator for the operations scenarios (stdlib only; mypy --strict clean, no typing.Any).

    python ci/loadgen.py submit  <api> <doc|docx|pdf> <variant>            -> prints the task id
    python ci/loadgen.py wait    <api> <task id> [timeout seconds]         -> prints the final state; exit 0 only on SUCCESS
    python ci/loadgen.py burst    <api> <n> [timeout seconds]               -> n different documents at once; prints a JSON summary; exit 1 on any failure
    python ci/loadgen.py soak     <api> <minutes> <requests per second> <out json>   -> steady mixed requests; polls each to its end; prints a JSON summary

A "variant" is an integer that changes an option which is part of the request key, so every variant is a different document (a cache miss),
while repeating a variant is a cache hit. The soak mixes new documents (a miss: git, core, Python, files) and repeats (a hit) the way DOC's traffic does.
"""

from __future__ import annotations

import json
import sys
import threading
import time
import urllib.error
import urllib.request
from dataclasses import dataclass, field

BOOKS = ["jud", "3jn", "2jn", "phm", "tit", "2th", "col", "1th", "php", "gal"]
TYPES = ["ulb", "tn", "tq", "tw"]
PATH = {"doc": "/documents", "docx": "/documents_docx", "pdf": "/documents"}


def body(kind: str, variant: int) -> dict[str, object]:
    """Variant n: the book rotates through the short ones, and the option set toggles with the higher bits, so n gives about 40 different keys."""
    book = BOOKS[variant % len(BOOKS)]
    types = TYPES[: 2 + (variant // len(BOOKS)) % 3]
    return {
        "email_address": None,
        "assembly_strategy_kind": "lbo",
        "assembly_layout_kind": "1c",
        "layout_for_print": (variant // 30) % 2 == 1,
        "generate_pdf": kind == "pdf",
        "generate_epub": False,
        "generate_docx": kind == "docx",
        "limit_words": True,
        "document_request_source": "ui",
        "resource_requests": [{"lang_code": "en", "resource_type": t, "book_code": book} for t in types],
    }


def call(url: str, data: dict[str, object] | None = None, timeout: float = 30) -> tuple[int, object]:
    request = urllib.request.Request(url, data=json.dumps(data).encode() if data is not None else None, headers={"content-type": "application/json"})
    try:
        with urllib.request.urlopen(request, timeout=timeout) as response:  # noqa: S310 (a URL given on the command line, http to the stack under test)
            return response.status, json.loads(response.read().decode() or "null")
    except urllib.error.HTTPError as e:
        return e.code, None
    except (urllib.error.URLError, TimeoutError, ConnectionError, json.JSONDecodeError):
        return 0, None


def submit(api: str, kind: str, variant: int) -> str:
    for _ in range(20):
        status, payload = call(api + PATH[kind], body(kind, variant))
        if status == 200 and isinstance(payload, dict) and isinstance(payload.get("task_id"), str):
            return str(payload["task_id"])
        time.sleep(1)
    raise SystemExit(f"submit failed for {kind} variant {variant}")


def state_of(api: str, task_id: str) -> tuple[str, object]:
    status, payload = call(f"{api}/task_status/{task_id}")
    if status == 200 and isinstance(payload, dict) and isinstance(payload.get("state"), str):
        return str(payload["state"]), payload.get("result")
    return "UNREACHABLE", None


def wait(api: str, task_id: str, timeout: float) -> tuple[str, object, float]:
    started = time.time()
    state: str = "UNREACHABLE"
    result: object = None
    while time.time() - started < timeout:
        state, result = state_of(api, task_id)
        if state in ("SUCCESS", "FAILURE"):
            break
        time.sleep(0.5)
    return state, result, time.time() - started


@dataclass
class Tally:
    lock: threading.Lock = field(default_factory=threading.Lock)
    done: int = 0
    failed: int = 0
    seconds: list[float] = field(default_factory=list)
    failures: list[str] = field(default_factory=list)

    def add(self, state: str, result: object, seconds: float) -> None:
        with self.lock:
            self.done += 1
            self.seconds.append(seconds)
            if state != "SUCCESS":
                self.failed += 1
                if len(self.failures) < 10:
                    self.failures.append(f"{state}: {json.dumps(result)[:300]}")


def percentile(values: list[float], q: float) -> float:
    ordered = sorted(values)
    return round(ordered[min(len(ordered) - 1, int(q * len(ordered)))], 2) if ordered else 0.0


def summary(tally: Tally, wall: float) -> dict[str, object]:
    return {"documents": tally.done, "failed": tally.failed, "wall_seconds": round(wall, 1), "p50_s": percentile(tally.seconds, 0.5), "p95_s": percentile(tally.seconds, 0.95), "max_s": percentile(tally.seconds, 1.0), "failures": tally.failures}


def one(api: str, kind: str, variant: int, tally: Tally, timeout: float) -> None:
    task = submit(api, kind, variant)
    state, result, seconds = wait(api, task, timeout)
    tally.add(state, result, seconds)


def burst(api: str, n: int, timeout: float) -> int:
    tally = Tally()
    started = time.time()
    threads = [threading.Thread(target=one, args=(api, ("docx", "doc")[i % 2], i, tally, timeout)) for i in range(n)]
    for t in threads:
        t.start()
    for t in threads:
        t.join()
    print(json.dumps(summary(tally, time.time() - started)))
    return 1 if tally.failed else 0


def soak(api: str, minutes: float, rate: float, out: str) -> int:
    tally = Tally()
    started = time.time()
    threads: list[threading.Thread] = []
    i = 0
    while time.time() - started < minutes * 60:
        # three in four requests repeat an earlier document (a cache hit); one in four is a new one (a miss), formats rotate
        variant = i // 4 if i % 4 == 0 else (i // 4) - (i % 4)
        kind = ("doc", "docx", "pdf", "doc")[i % 4]
        t = threading.Thread(target=one, args=(api, kind, max(variant, 0), tally, 600))
        t.start()
        threads.append(t)
        i += 1
        time.sleep(1.0 / rate)
    for t in threads:
        t.join()
    result = summary(tally, time.time() - started)
    result["requests_sent"] = i
    with open(out, "w", encoding="utf-8") as f:
        json.dump(result, f, indent=2)
    print(json.dumps(result))
    return 1 if tally.failed else 0


def main(argv: list[str]) -> int:
    cmd, api = argv[1], argv[2].rstrip("/")
    if cmd == "submit":
        print(submit(api, argv[3], int(argv[4])))
        return 0
    if cmd == "wait":
        state, result, seconds = wait(api, argv[3], float(argv[4]) if len(argv) > 4 else 300)
        print(json.dumps({"state": state, "result": result, "seconds": round(seconds, 1)}))
        return 0 if state == "SUCCESS" else 1
    if cmd == "burst":
        return burst(api, int(argv[3]), float(argv[4]) if len(argv) > 4 else 600)
    if cmd == "soak":
        return soak(api, float(argv[3]), float(argv[4]), argv[5])
    raise SystemExit(__doc__)


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
