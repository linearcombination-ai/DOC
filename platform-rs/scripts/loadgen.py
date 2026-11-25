"""Load generation for the P6 evidence: a soak of mixed requests with memory sampled, a burst of 20+ concurrent
jobs, and a cold run against the real catalog and the real content host.

    python3 -I loadgen.py soak  --bin B --server-bin S --out DIR [--minutes 30] [--daemons 2]
    python3 -I loadgen.py burst --bin B --server-bin S --out DIR [--jobs 24]
    python3 -I loadgen.py real  --bin B --server-bin S --out DIR

Offline modes (soak, burst) use the fixture repositories through `scripts/fake-git.sh` and a local catalog, so the numbers
are the service's own cost (no network). `real` uses the BIEL data API and the real git host.
Host load is recorded with every sample; the soak refuses to start while the 1-minute load is above 3.
"""

from __future__ import annotations

import argparse
import csv
import http.server
import json
import os
import random
import shutil
import signal
import socket
import statistics
import subprocess
import threading
import time
import urllib.error
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BASE = "https://content.example.org/WycliffeAssociates"
REPOS = [("en_ulb", "ulb", "en", "English"), ("en_tn", "tn", "en", "English"), ("en_tn_condensed", "tn-condensed", "en", "English"), ("en_tw", "tw", "en", "English"), ("en_bc", "bc", "en", "English"),
         ("es-419_ulb", "ulb", "es-419", "Spanish"), ("es-419_tn", "tn", "es-419", "Spanish"), ("es-419_tq", "tq", "es-419", "Spanish")]
CATALOG = json.dumps({"data": {"git_repo": [{"repo_url": f"{BASE}/{n}", "content": {"resource_type": rt, "language": {"english_name": en, "ietf_code": lc, "national_name": en, "direction": "ltr"}}} for n, rt, lc, en in REPOS]}}).encode()

Json = dict[str, object]


class CatalogHandler(http.server.BaseHTTPRequestHandler):
    def do_POST(self) -> None:
        self.rfile.read(int(self.headers.get("content-length", "0")))
        self.send_response(200)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(CATALOG)))
        self.end_headers()
        self.wfile.write(CATALOG)

    def log_message(self, format: str, *args: object) -> None:  # noqa: A002
        return


def free_port() -> int:
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return int(s.getsockname()[1])


def rr(lang: str, rt: str, book: str = "jud") -> Json:
    return {"lang_code": lang, "resource_type": rt, "book_code": book}


# (name, strategy, layout, resources, extra) - the request shapes the UI can post, with docx and html outputs
SHAPES: list[tuple[str, str, str | None, list[Json], Json]] = [
    ("ulb", "lbo", None, [rr("en", "ulb")], {}),
    ("ulb+tn", "lbo", None, [rr("en", "ulb"), rr("en", "tn")], {}),
    ("ulb+tn blo", "blo", None, [rr("en", "ulb"), rr("en", "tn")], {}),
    ("ulb+tn+bc", "lbo", None, [rr("en", "ulb"), rr("en", "tn"), rr("en", "bc")], {}),
    ("tn", "lbo", None, [rr("en", "tn")], {}),
    ("ulb+tw", "lbo", None, [rr("en", "ulb"), rr("en", "tw")], {}),
    ("ulb+tw include", "lbo", None, [rr("en", "ulb"), rr("en", "tw")], {"tw_mode": "include"}),
    ("es ulb+tq", "lbo", None, [rr("es-419", "ulb"), rr("es-419", "tq")], {}),
    ("en+es lbo", "lbo", None, [rr("en", "ulb"), rr("en", "tn"), rr("es-419", "ulb"), rr("es-419", "tn")], {}),
    ("en+es 2col", "blo", "2c_sl_sr", [rr("en", "ulb"), rr("es-419", "ulb")], {}),
]


def request(shape: int, docx: bool, bits: int) -> Json:
    name, strategy, layout, resources, extra = SHAPES[shape]
    body: Json = {
        "assembly_strategy_kind": strategy, "document_request_source": "ui", "generate_docx": docx, "generate_pdf": False,
        "layout_for_print": bool(bits & 1), "use_chapter_labels": bool(bits & 2), "show_tn_book_intro": not bits & 4,
        "use_section_visual_separator": bool(bits & 8), "show_bc_book_intro": not bits & 16, "show_tn_chapter_intro": not bits & 32,
        "resource_requests": resources,
    }
    if layout:
        body["assembly_layout_kind"] = layout
    body.update(extra)
    return body


def http_json(url: str, body: Json | None = None, timeout: float = 30) -> tuple[int, Json, dict[str, str]]:
    data = None if body is None else json.dumps(body).encode()
    req = urllib.request.Request(url, data, {"content-type": "application/json"} if body is not None else {})
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r:
            parsed: object = json.load(r)
            return int(r.status), (dict(parsed) if isinstance(parsed, dict) else {"value": parsed}), {k.lower(): v for k, v in r.headers.items()}
    except urllib.error.HTTPError as e:
        raw = e.read()
        try:
            out: Json = dict(json.loads(raw))
        except ValueError:
            out = {"raw": raw.decode(errors="replace")[:200]}
        return int(e.code), out, {k.lower(): v for k, v in e.headers.items()}


def run_job(base: str, body: Json, docx: bool, timeout: float = 900) -> tuple[str, float, str, bool]:
    """Returns (state, seconds, detail, coalesced)."""
    t0 = time.perf_counter()
    status, resp, headers = http_json(f"{base}/{'documents_docx' if docx else 'documents'}", body)
    if status != 200:
        return f"HTTP{status}", time.perf_counter() - t0, str(resp.get("code", resp.get("detail", "")))[:80], False
    tid = str(resp["task_id"])
    coalesced = headers.get("x-doc-coalesced") == "true"
    while True:
        _, s, _ = http_json(f"{base}/task_status/{tid}")
        st = str(s.get("state"))
        if st in ("SUCCESS", "FAILURE"):
            detail = ""
            if st == "FAILURE":
                r = s.get("result")
                detail = str(r.get("code")) if isinstance(r, dict) else ""
            return st, time.perf_counter() - t0, detail, coalesced
        if time.perf_counter() - t0 > timeout:
            return "TIMEOUT", time.perf_counter() - t0, st, coalesced
        time.sleep(0.05)


def status_value(pid: int, field: str) -> int:
    try:
        for line in Path(f"/proc/{pid}/status").read_text().splitlines():
            if line.startswith(field):
                return int(line.split()[1])
    except OSError:
        pass
    return 0


def descendants(pid: int) -> list[int]:
    kids: dict[int, list[int]] = {}
    for d in Path("/proc").iterdir():
        if d.name.isdigit():
            try:
                stat = (d / "stat").read_text()
            except OSError:
                continue
            ppid = int(stat.rsplit(")", 1)[1].split()[1])
            kids.setdefault(ppid, []).append(int(d.name))
    out: list[int] = []
    todo = [pid]
    while todo:
        for k in kids.get(todo.pop(), []):
            out.append(k)
            todo.append(k)
    return out


class Stack:
    def __init__(self, a: argparse.Namespace, daemons: int, extra: dict[str, str], real: bool) -> None:
        self.work = Path(a.out) / "work"
        shutil.rmtree(self.work, ignore_errors=True)
        self.work.mkdir(parents=True)
        self.real = real
        self.procs: list[subprocess.Popen[bytes]] = []
        self.bases: list[str] = []
        self.pids: list[int] = []
        cat_url = "https://api.bibleineverylanguage.org/v1/graphql"
        if not real:
            cport = free_port()
            self.catalog_srv = http.server.ThreadingHTTPServer(("127.0.0.1", cport), CatalogHandler)
            threading.Thread(target=self.catalog_srv.serve_forever, daemon=True).start()
            cat_url = f"http://127.0.0.1:{cport}/v1/graphql"
        rport = free_port()
        self.server = subprocess.Popen([a.server_bin, "--port", str(rport), "--bind", "127.0.0.1", "--dir", str(self.work), "--save", "", "--appendonly", "yes", "--appendfsync", "everysec"], stdout=subprocess.DEVNULL)
        time.sleep(0.5)
        for i in range(daemons):
            port = free_port()
            env = {
                "PATH": os.environ["PATH"], "DATA_API_URL": cat_url, "DOC_SHELL_ADDR": f"127.0.0.1:{port}", "DOC_SHELL_MODE": "fixed", "DOC_SHELL_LOG_FORMAT": "json",
                "DOC_SHELL_CACHE_DIR": str(self.work / "cache"), "DOC_JOBD_REDIS_URL": f"redis://127.0.0.1:{rport}/0", "DOC_JOBD_NAMESPACE": "load",
                "DOC_JOBD_OUTPUT_DIR": str(self.work / "out"), "DOC_JOBD_DOCX_WORKER": str(ROOT / "python/docx_worker.py"), "DOC_TEMPLATES_DIR": str(ROOT.parent / "backend/templates/html"),
                "DOC_JOBD_WORKER_ID": f"load-{i}", "RUST_LOG": "info", "DOC_JOBD_BEAT_EVERY_MS": "2000",
            }
            if not real:
                env.update({"DOC_SHELL_GIT_BIN": str(ROOT / "scripts/fake-git.sh"), "FAKEGIT_CONTENT": str(ROOT / "fixtures/content")})
            env.update(extra)
            p = subprocess.Popen([a.bin], env=env, stdout=(self.work / f"daemon{i}.log").open("wb"), stderr=subprocess.STDOUT)
            self.procs.append(p)
            self.pids.append(p.pid)
            base = f"http://127.0.0.1:{port}"
            self.bases.append(base)
            for _ in range(300):
                try:
                    urllib.request.urlopen(f"{base}/health/status", timeout=1).read()
                    break
                except OSError:
                    time.sleep(0.1)

    def rss(self) -> tuple[int, int, int]:
        parent = sum(status_value(p, "VmRSS:") for p in self.pids)
        kids = sum(status_value(k, "VmRSS:") for p in self.pids for k in descendants(p))
        return parent, kids, status_value(self.server.pid, "VmRSS:")

    def stop(self) -> list[int]:
        codes: list[int] = []
        for p in self.procs:
            p.send_signal(signal.SIGTERM)
        for p in self.procs:
            try:
                codes.append(p.wait(timeout=120))
            except subprocess.TimeoutExpired:
                p.kill()
                codes.append(-9)
        self.server.terminate()
        self.server.wait()
        return codes


def pct(xs: list[float], p: float) -> float:
    s = sorted(xs)
    return s[min(len(s) - 1, int(round(p * (len(s) - 1))))] if s else 0.0


def slope_kb_per_min(points: list[tuple[float, int]]) -> float:
    if len(points) < 3:
        return 0.0
    xs = [t for t, _ in points]
    ys = [float(v) for _, v in points]
    mx, my = statistics.fmean(xs), statistics.fmean(ys)
    den = sum((x - mx) ** 2 for x in xs)
    return 0.0 if den == 0 else sum((x - mx) * (y - my) for x, y in zip(xs, ys)) / den * 60.0


def soak(a: argparse.Namespace) -> int:
    load = os.getloadavg()[0]
    if load > 3.0 and not a.force:
        print(f"host load {load:.2f} is above 3: refusing to start the soak (use --force to override and record it)")
        return 3
    out = Path(a.out)
    out.mkdir(parents=True, exist_ok=True)
    stack = Stack(a, a.daemons, {"DOC_DOCX_MODE": "persistent", "DOC_DOCX_RECYCLE_AFTER": "300", "DOC_JOBD_CONCURRENCY": "3", "ASSET_CACHING_PERIOD": "2", "DOC_JOBD_OUTPUT_GRACE_MS": "60000", "DOC_JOBD_JANITOR_EVERY_MS": "30000"}, False)
    rows: list[Json] = []
    lock = threading.Lock()
    stop_at = time.time() + a.minutes * 60
    rnd = random.Random(20261125)
    counters = {"ok": 0, "failed": 0, "coalesced": 0, "http_err": 0}
    failures: dict[str, int] = {}
    lat: list[float] = []

    def worker(wid: int) -> None:
        r = random.Random(wid * 7919)
        n = 0
        recent: list[tuple[int, bool, int]] = []
        while time.time() < stop_at:
            n += 1
            base = stack.bases[(wid + n) % len(stack.bases)]
            if recent and n % 7 == 0:
                shape, docx, bits = r.choice(recent)  # a repeat: a cache hit while the document is fresh
            else:
                shape, docx, bits = r.randrange(len(SHAPES)), r.random() < 0.7, r.randrange(64)
                recent = (recent + [(shape, docx, bits)])[-20:]
            burst = 5 if n % 25 == 0 else 1
            results: list[tuple[str, float, str, bool]] = []

            def one() -> None:
                results.append(run_job(base, request(shape, docx, bits), docx))

            ts = [threading.Thread(target=one) for _ in range(burst)]
            for t in ts:
                t.start()
            for t in ts:
                t.join()
            with lock:
                for st, secs, detail, co in results:
                    lat.append(secs)
                    if st == "SUCCESS":
                        counters["ok"] += 1
                    elif st.startswith("HTTP"):
                        counters["http_err"] += 1
                        failures[f"{st}:{detail}"] = failures.get(f"{st}:{detail}", 0) + 1
                    else:
                        counters["failed"] += 1
                        failures[f"{st}:{detail}"] = failures.get(f"{st}:{detail}", 0) + 1
                    counters["coalesced"] += int(co)
            time.sleep(r.uniform(0.1, 0.6))

    threads = [threading.Thread(target=worker, args=(i,), daemon=True) for i in range(a.submitters)]
    t0 = time.time()
    for t in threads:
        t.start()
    samples: list[tuple[float, int]] = []
    with (out / "soak.csv").open("w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["t_s", "load1", "daemons_rss_kb", "children_rss_kb", "redis_rss_kb", "jobs_ok", "jobs_failed", "http_err", "coalesced", "out_files"])
        while time.time() < stop_at + 1 and any(t.is_alive() for t in threads):
            p, k, rd = stack.rss()
            files = len(list((stack.work / "out").glob("*"))) if (stack.work / "out").exists() else 0
            with lock:
                w.writerow([round(time.time() - t0, 1), os.getloadavg()[0], p, k, rd, counters["ok"], counters["failed"], counters["http_err"], counters["coalesced"], files])
            f.flush()
            samples.append((time.time() - t0, p))
            time.sleep(5)
    for t in threads:
        t.join(timeout=300)
    codes = stack.stop()
    warm = [(t, v) for t, v in samples if t > 300]
    first_third = [v for t, v in samples if 300 < t < 300 + (a.minutes * 60 - 300) / 3]
    last_third = [v for t, v in samples if t > a.minutes * 60 - (a.minutes * 60 - 300) / 3]
    summary: Json = {
        "minutes": a.minutes, "daemons": a.daemons, "submitters": a.submitters, "host_cpus": os.cpu_count(), "load_at_start": load, "load_at_end": os.getloadavg(),
        "jobs": dict(counters), "failure_kinds": failures, "latency_s": {"n": len(lat), "p50": pct(lat, 0.5), "p95": pct(lat, 0.95), "p99": pct(lat, 0.99), "max": max(lat or [0.0])},
        "daemon_rss_kb": {"first_sample": samples[0][1] if samples else 0, "max": max((v for _, v in samples), default=0), "last": samples[-1][1] if samples else 0,
                           "mean_first_third": statistics.fmean(first_third) if first_third else 0, "mean_last_third": statistics.fmean(last_third) if last_third else 0},
        "daemon_rss_slope_kb_per_min_after_5min": slope_kb_per_min(warm), "exit_codes_after_sigterm": codes,
    }
    (out / "soak-summary.json").write_text(json.dumps(summary, indent=1))
    print(json.dumps(summary, indent=1))
    return 0


def burst(a: argparse.Namespace) -> int:
    out = Path(a.out)
    out.mkdir(parents=True, exist_ok=True)
    stack = Stack(a, 2, {"DOC_DOCX_MODE": "persistent", "DOC_JOBD_CONCURRENCY": "8", "DOC_DOCX_POOL_SIZE": "8"}, False)
    warm = run_job(stack.bases[0], request(0, True, 0), True)
    res: dict[str, object] = {"load_before": os.getloadavg(), "warm_first_job_s": warm}
    for mode, docx in (("docx", True), ("html", False), ("mixed", None)):
        rss0 = stack.rss()
        t0 = time.perf_counter()
        results: list[tuple[str, float, str, bool]] = []
        lock = threading.Lock()

        def one(i: int, mode_docx: bool | None = docx) -> None:
            d = (i % 2 == 0) if mode_docx is None else mode_docx
            r = run_job(stack.bases[i % 2], request(i % len(SHAPES), d, 100 + i), d)
            with lock:
                results.append(r)

        ts = [threading.Thread(target=one, args=(i,)) for i in range(a.jobs)]
        peak = 0
        for t in ts:
            t.start()
        while any(t.is_alive() for t in ts):
            p, k, _ = stack.rss()
            peak = max(peak, p + k)
            time.sleep(0.1)
        for t in ts:
            t.join()
        wall = time.perf_counter() - t0
        res[mode] = {"jobs": a.jobs, "wall_s": wall, "succeeded": sum(1 for r in results if r[0] == "SUCCESS"), "failed": [r[2] for r in results if r[0] != "SUCCESS"],
                     "latency_p50_s": pct([r[1] for r in results], 0.5), "latency_max_s": max(r[1] for r in results), "peak_rss_daemons_plus_children_kb": peak, "rss_before_kb": rss0[0] + rss0[1]}
        print(mode, json.dumps(res[mode]))
    res["load_after"] = os.getloadavg()
    res["exit_codes"] = stack.stop()
    (out / "burst.json").write_text(json.dumps(res, indent=1))
    return 0


def real(a: argparse.Namespace) -> int:
    out = Path(a.out)
    out.mkdir(parents=True, exist_ok=True)
    stack = Stack(a, 1, {"DOC_DOCX_MODE": "persistent", "DOC_SHELL_CLONE_TIMEOUT_SECS": "600"}, True)
    res: dict[str, object] = {"load_before": os.getloadavg(), "catalog": "https://api.bibleineverylanguage.org/v1/graphql", "git_host": "https://content.bibletranslationtools.org"}
    body: Json = {"assembly_strategy_kind": "lbo", "document_request_source": "ui", "generate_docx": True, "generate_pdf": False, "resource_requests": [rr("en", "ulb"), rr("en", "tn")]}
    t0 = time.perf_counter()
    state, secs, detail, _ = run_job(stack.bases[0], body, True, timeout=1500)
    res["cold"] = {"state": state, "seconds": secs, "detail": detail}
    html_body = dict(body)
    html_body["generate_docx"] = False
    res["html_after_cold"] = dict(zip(("state", "seconds", "detail", "coalesced"), run_job(stack.bases[0], html_body, False, timeout=300)))
    body2 = dict(body)
    body2["layout_for_print"] = True
    res["warm_distinct_key"] = dict(zip(("state", "seconds", "detail", "coalesced"), run_job(stack.bases[0], body2, True, timeout=300)))
    res["cache_hit"] = dict(zip(("state", "seconds", "detail", "coalesced"), run_job(stack.bases[0], body, True, timeout=300)))
    res["total_s"] = time.perf_counter() - t0
    res["rss_kb"] = stack.rss()
    _, hs, _ = http_json(f"{stack.bases[0]}/health/status")
    res["health"] = hs
    res["load_after"] = os.getloadavg()
    res["exit_codes"] = stack.stop()
    (out / "real-network.json").write_text(json.dumps(res, indent=1))
    print(json.dumps(res, indent=1))
    return 0


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("mode", choices=["soak", "burst", "real"])
    ap.add_argument("--bin", required=True)
    ap.add_argument("--server-bin", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--minutes", type=float, default=30)
    ap.add_argument("--daemons", type=int, default=2)
    ap.add_argument("--submitters", type=int, default=4)
    ap.add_argument("--jobs", type=int, default=24)
    ap.add_argument("--force", action="store_true")
    a = ap.parse_args()
    return {"soak": soak, "burst": burst, "real": real}[a.mode](a)


if __name__ == "__main__":
    raise SystemExit(main())
