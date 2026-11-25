"""A fake SMTP server that prints each message it receives (headers and attachment names), for the email demo.

    python3 -I fake_smtp.py --port 2525
"""

from __future__ import annotations

import argparse
import socketserver


class Handler(socketserver.StreamRequestHandler):
    def handle(self) -> None:
        w = self.wfile
        w.write(b"220 fake ESMTP\r\n")
        data_mode = False
        lines: list[str] = []
        for raw in self.rfile:
            line = raw.decode("utf-8", "replace")
            if data_mode:
                if line == ".\r\n":
                    data_mode = False
                    head = [x.rstrip() for x in lines if x.split(":")[0] in ("From", "To", "Subject", "Message-ID") or "filename=" in x]
                    print("MESSAGE RECEIVED:", *head, sep="\n  ", flush=True)
                    w.write(b"250 queued\r\n")
                    lines = []
                else:
                    lines.append(line)
                continue
            up = line.strip().upper()
            if up.startswith("EHLO"):
                w.write(b"250-fake\r\n250 8BITMIME\r\n")
            elif up == "DATA":
                data_mode = True
                w.write(b"354 go ahead\r\n")
            elif up == "QUIT":
                w.write(b"221 bye\r\n")
                return
            else:
                w.write(b"250 ok\r\n")


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, default=2525)
    a = ap.parse_args()
    socketserver.ThreadingTCPServer.allow_reuse_address = True
    with socketserver.ThreadingTCPServer(("127.0.0.1", a.port), Handler) as srv:
        print(f"fake SMTP on 127.0.0.1:{a.port}", flush=True)
        srv.serve_forever()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
