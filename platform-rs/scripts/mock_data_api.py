"""A local mirror of the BIEL data API (GraphQL), for when the real one is unreachable or a run must be repeatable.

    python3 -I mock_data_api.py --port 8099 [--snapshot FILE.json] [--fail-after N]

Answers every POST with the catalog: the fixture repositories (English and Spanish, Jude only) by default, or a
recorded real answer (`--snapshot`). `--down` makes it answer 503 (to demonstrate the typed 503); SIGUSR1 toggles it.
"""

from __future__ import annotations

import argparse
import http.server
import json
import signal
from pathlib import Path
from types import FrameType

BASE = "https://content.example.org/WycliffeAssociates"
REPOS = [("en_ulb", "ulb", "en", "English"), ("en_tn", "tn", "en", "English"), ("en_tn_condensed", "tn-condensed", "en", "English"), ("en_tw", "tw", "en", "English"), ("en_bc", "bc", "en", "English"),
         ("es-419_ulb", "ulb", "es-419", "Spanish"), ("es-419_tn", "tn", "es-419", "Spanish"), ("es-419_tq", "tq", "es-419", "Spanish")]


def fixture_catalog() -> bytes:
    rows = [{"repo_url": f"{BASE}/{n}", "content": {"resource_type": rt, "language": {"english_name": en, "ietf_code": lc, "national_name": en, "direction": "ltr"}}} for n, rt, lc, en in REPOS]
    return json.dumps({"data": {"git_repo": rows}}).encode()


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, default=8099)
    ap.add_argument("--snapshot", type=Path)
    ap.add_argument("--down", action="store_true")
    a = ap.parse_args()
    body = a.snapshot.read_bytes() if a.snapshot else fixture_catalog()
    state = {"down": a.down, "calls": 0}

    class H(http.server.BaseHTTPRequestHandler):
        def do_POST(self) -> None:
            self.rfile.read(int(self.headers.get("content-length", "0")))
            state["calls"] += 1
            code = 503 if state["down"] else 200
            payload = b"" if state["down"] else body
            self.send_response(code)
            self.send_header("content-type", "application/json")
            self.send_header("content-length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)

        def log_message(self, format: str, *args: object) -> None:  # noqa: A002
            return

    def toggle(_sig: int, _frame: FrameType | None) -> None:
        state["down"] = not state["down"]
        print(f"data API mirror is now {'DOWN (503)' if state['down'] else 'UP'}", flush=True)

    signal.signal(signal.SIGUSR1, toggle)
    srv = http.server.ThreadingHTTPServer(("127.0.0.1", a.port), H)
    print(f"data API mirror on http://127.0.0.1:{a.port}/v1/graphql ({'recorded snapshot' if a.snapshot else 'fixture catalog'}); SIGUSR1 toggles 503", flush=True)
    srv.serve_forever()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
