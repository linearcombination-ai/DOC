"""Latency and memory of the P5 slice: one request (en ulb + tn, Jude, docx) through doc-jobd on this host.

    python3 -I bench.py --bin PATH --redis-bin PATH --out results.json [--runs 20] [--real-network]

Scenarios (each a fresh doc-jobd process unless noted):
  cold_process   first request after start: catalog fetch, repository acquisition, parse, assemble, Python cold start
  warm_sequential N further requests with distinct cache keys (a setting that changes the key), repositories cached
  concurrent4    4 distinct requests at once, worker concurrency 4
  cache_hit      the same request again (a fresh document is served again)
Everything but git and the catalog is real; git is `scripts/fake-git.sh` over fixture content and the catalog is a
local HTTP server, so the numbers are compute and seam cost, not network. With --real-network the catalog is the
BIEL data API and git is real (one cold run only).
"""

from __future__ import annotations

import argparse
import http.server
import json
import os
import shutil
import signal
import socket
import statistics
import subprocess
import tempfile
import threading
import time
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CATALOG = json.dumps(
    {"data": {"git_repo": [
        {"repo_url": "https://content.example.org/WycliffeAssociates/en_ulb", "content": {"resource_type": "ulb", "language": {"english_name": "English", "ietf_code": "en", "national_name": "English", "direction": "ltr"}}},
        {"repo_url": "https://content.example.org/WycliffeAssociates/en_tn", "content": {"resource_type": "tn", "language": {"english_name": "English", "ietf_code": "en", "national_name": "English", "direction": "ltr"}}},
    ]}}
).encode()


class Catalog(http.server.BaseHTTPRequestHandler):
    def do_POST(self) -> None:
        self.rfile.read(int(self.headers.get("content-length", "0")))
        self.send_response(200)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(CATALOG)))
        self.end_headers()
        self.wfile.write(CATALOG)

    def log_message(self, *_: object) -> None:
        return


def free_port() -> int:
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return int(s.getsockname()[1])


def post(base: str, body: dict[str, object]) -> str:
    req = urllib.request.Request(f"{base}/documents_docx", json.dumps(body).encode(), {"content-type": "application/json"})
    with urllib.request.urlopen(req, timeout=30) as r:
        return str(json.load(r)["task_id"])


def state(base: str, tid: str) -> dict[str, object]:
    with urllib.request.urlopen(f"{base}/task_status/{tid}", timeout=30) as r:
        out: dict[str, object] = json.load(r)
        return out


def run_job(base: str, body: dict[str, object], timeout: float = 600) -> float:
    t0 = time.perf_counter()
    tid = post(base, body)
    while True:
        s = state(base, tid)
        if s["state"] in ("SUCCESS", "FAILURE"):
            if s["state"] != "SUCCESS":
                raise RuntimeError(f"job failed: {s}")
            return time.perf_counter() - t0
        if time.perf_counter() - t0 > timeout:
            raise TimeoutError(str(s))
        time.sleep(0.002)


def request(variant: int) -> dict[str, object]:
    # `use_chapter_labels` and friends are hashed into the key: different bits, different key, same content
    return {
        "assembly_strategy_kind": "lbo", "document_request_source": "ui", "generate_docx": True, "generate_pdf": False,
        "use_chapter_labels": bool(variant & 1), "layout_for_print": bool(variant & 2), "show_tn_book_intro": not variant & 4,
        "use_section_visual_separator": bool(variant & 8), "show_bc_book_intro": not variant & 16,
        "resource_requests": [{"lang_code": "en", "resource_type": "ulb", "book_code": "jud"}, {"lang_code": "en", "resource_type": "tn", "book_code": "jud"}],
    }


def rss_kb(pid: int, field: str) -> int:
    for line in Path(f"/proc/{pid}/status").read_text().splitlines():
        if line.startswith(field):
            return int(line.split()[1])
    return 0


def num(v: object) -> float:
    return float(v) if isinstance(v, (int, float)) else 0.0


def stage_log(log: Path) -> dict[str, list[float]]:
    out: dict[str, list[float]] = {}
    for line in log.read_text().splitlines():
        try:
            rec: object = json.loads(line)
        except ValueError:
            continue
        if not isinstance(rec, dict):
            continue
        nested = rec.get("fields")  # the P6 daemon logs flattened JSON; P5 nested it under "fields"
        f: dict[str, object] = {str(k): v for k, v in (nested if isinstance(nested, dict) else rec).items()}
        msg = f.get("message")
        if msg == "stage finished" and f.get("ok"):
            out.setdefault(str(f["stage"]), []).append(num(f.get("ms")))
        if msg in ("docx worker finished", "docx worker finished (persistent)"):
            out.setdefault("docx_wall", []).append(num(f.get("wall_ms")))
            out.setdefault("docx_worker_ms", []).append(num(f.get("worker_ms")))
            out.setdefault("docx_maxrss_kb", []).append(num(f.get("maxrss_kb")) or num(f.get("rss_kb")))
        if msg == "document placed":
            out.setdefault("job_total", []).append(num(f.get("total_ms")))
    return out


def pct(xs: list[float], p: float) -> float:
    s = sorted(xs)
    return s[min(len(s) - 1, int(round(p * (len(s) - 1))))]


def summary(xs: list[float]) -> dict[str, float]:
    return {"n": len(xs), "min": min(xs), "p50": statistics.median(xs), "p95": pct(xs, 0.95), "max": max(xs), "mean": statistics.fmean(xs)}


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--bin", required=True)
    ap.add_argument("--redis-bin", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--runs", type=int, default=20)
    ap.add_argument("--real-network", action="store_true")
    ap.add_argument("--docx-mode", default="per_job", choices=["per_job", "persistent"])
    a = ap.parse_args()
    work = Path(os.environ.get("BENCH_TMP", str(ROOT / "scratch/bench")))
    shutil.rmtree(work, ignore_errors=True)
    work.mkdir(parents=True)
    result: dict[str, object] = {"load_before": os.getloadavg(), "host_cpus": os.cpu_count()}
    cat_port = free_port()
    srv = http.server.ThreadingHTTPServer(("127.0.0.1", cat_port), Catalog)
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    redis_port = free_port()
    redis = subprocess.Popen([a.redis_bin, "--port", str(redis_port), "--bind", "127.0.0.1", "--dir", str(work), "--save", "", "--appendonly", "yes", "--appendfsync", "everysec"], stdout=subprocess.DEVNULL)
    time.sleep(0.4)

    def start(tag: str, extra: dict[str, str]) -> tuple[subprocess.Popen[bytes], str, Path, Path]:
        port = free_port()
        out = work / f"out-{tag}"
        log = work / f"{tag}.log"
        env = {
            "PATH": os.environ["PATH"], "DATA_API_URL": f"http://127.0.0.1:{cat_port}/v1/graphql", "DOC_SHELL_ADDR": f"127.0.0.1:{port}",
            "DOC_SHELL_GIT_BIN": str(ROOT / "scripts/fake-git.sh"), "FAKEGIT_CONTENT": str(ROOT / "fixtures/content"), "DOC_SHELL_CACHE_DIR": str(work / f"cache-{tag}"),
            "DOC_SHELL_MODE": "fixed", "DOC_SHELL_LOG_FORMAT": "json", "DOC_JOBD_REDIS_URL": f"redis://127.0.0.1:{redis_port}/0", "DOC_JOBD_NAMESPACE": tag,
            "DOC_JOBD_OUTPUT_DIR": str(out), "DOC_JOBD_DOCX_WORKER": str(ROOT / "python/docx_worker.py"), "DOC_JOBD_CLAIM_BLOCK_MS": "200", "DOC_JOBD_PROMOTE_EVERY_MS": "200",
            "RUST_LOG": "info", "DOC_DOCX_MODE": a.docx_mode, "DOC_TEMPLATES_DIR": str(ROOT.parent / "backend/templates/html"),
        }
        env.update(extra)
        p = subprocess.Popen([a.bin], env=env, stdout=log.open("wb"), stderr=subprocess.STDOUT)
        base = f"http://127.0.0.1:{port}"
        for _ in range(200):
            try:
                urllib.request.urlopen(f"{base}/health/status", timeout=1).read()
                break
            except OSError:
                time.sleep(0.05)
        return p, base, log, out

    def stop(p: subprocess.Popen[bytes]) -> int:
        p.send_signal(signal.SIGTERM)
        try:
            return p.wait(timeout=60)
        except subprocess.TimeoutExpired:
            p.kill()
            return -9

    # cold process, then warm sequential, same process
    p, base, log, out = start("seq", {})
    idle_rss = rss_kb(p.pid, "VmRSS:")
    result["idle_rss_kb"] = idle_rss
    cold = run_job(base, request(0))
    result["cold_process_s"] = cold
    result["rss_after_first_job_kb"] = rss_kb(p.pid, "VmRSS:")
    warm = [run_job(base, request(v)) for v in range(1, a.runs + 1)]
    result["warm_sequential_s"] = summary(warm)
    result["cache_hit_s"] = summary([run_job(base, request(0)) for _ in range(10)])
    result["rss_after_warm_kb"] = rss_kb(p.pid, "VmRSS:")
    result["peak_rss_kb"] = rss_kb(p.pid, "VmHWM:")
    rc = stop(p)
    result["exit_code_after_sigterm"] = rc
    st = stage_log(log)
    result["stages_ms"] = {k: summary(v) for k, v in st.items()}
    result["first_job_stages_ms"] = {k: v[0] for k, v in st.items()}

    # concurrent
    p, base, log, out = start("conc", {"DOC_JOBD_CONCURRENCY": "4"})
    run_job(base, request(0))
    t0 = time.perf_counter()
    done: list[float] = []
    threads = [threading.Thread(target=lambda v=v: done.append(run_job(base, request(v)))) for v in range(1, 5)]
    for t in threads:
        t.start()
    for t in threads:
        t.join()
    result["concurrent4"] = {"wall_s": time.perf_counter() - t0, "each_s": sorted(done), "peak_rss_kb": rss_kb(p.pid, "VmHWM:")}
    stop(p)
    redis.terminate()
    redis.wait()
    result["load_after"] = os.getloadavg()
    Path(a.out).write_text(json.dumps(result, indent=1))
    print(json.dumps(result, indent=1))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
