"""The Python seam as a container service: docx and PDF over HTTP, one subprocess per job.

The Rust worker writes a job file on the shared volume and POSTs its path here; this service runs the existing
per-job scripts (``docx_worker.py --job FILE``, ``pdf_worker.py --job FILE``) in their own process group, enforces
the deadline by killing that group, and answers with the exit status, the script's one JSON line and a tail of its
log. Nothing about the job formats changed: the same scripts run in the same way as in the local seam; only the
process boundary moved to a container boundary.

    POST /v1/docx   {"job_file": "...", "deadline_ms": 300000, "request_id": "..."}
    POST /v1/pdf    {"job_file": "...", "deadline_ms": 600000, "request_id": "...", "engine": "prince" | "weasyprint"}
    GET  /healthz   liveness (the process answers)
    GET  /readyz    readiness: the scripts exist, which PDF engines can run (Prince, WeasyPrint)
    GET  /metrics   Prometheus text: jobs by kind and outcome, in flight, engine fallbacks

Answer (200 whenever a subprocess ran, even if it failed; the caller maps the exit status): ``exit`` (int or null when
killed by a signal), ``signal``, ``timed_out``, ``stdout``, ``stderr`` (tails), ``pid``, ``wall_ms``, ``engine`` and
``fallback`` (PDF: Prince was asked for and WeasyPrint produced the file). 400 for a malformed request, 503 while
draining, 500 only for a bug here. The job file must be inside ``PYWORKER_ROOT`` (default the output volume).

Environment: PYWORKER_PORT (9100), PYWORKER_HOST (0.0.0.0), PYWORKER_CONCURRENCY (2), PYWORKER_ROOT
(/app/document_output), PYWORKER_DRAIN_SECONDS (120), PYWORKER_DOCX_SCRIPT, PYWORKER_PDF_SCRIPT. SIGTERM stops
accepting work, lets running jobs finish (up to the drain time) and exits 0.
"""

from __future__ import annotations

import json
import os
import shutil
import signal
import subprocess
import sys
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

HERE = Path(__file__).resolve().parent
MAX_BODY = 64 * 1024
TAIL_BYTES = 8 * 1024

Json = dict[str, object]


def _log(level: str, event: str, request_id: str = "", **fields: str | int | float | bool | None) -> None:
    rec: dict[str, str | int | float | bool | None] = {"ts": round(time.time(), 3), "level": level, "event": event, "service": "pyworker"}
    if request_id:
        rec["request_id"] = request_id
    rec.update(fields)
    sys.stderr.write(json.dumps(rec) + "\n")
    sys.stderr.flush()


def _tail(data: bytes) -> str:
    return data[-TAIL_BYTES:].decode("utf-8", errors="replace")


class Metrics:
    """Counters behind one lock; rendered as Prometheus text."""

    def __init__(self) -> None:
        self._lock = threading.Lock()
        self._jobs: dict[tuple[str, str], int] = {}
        self._inflight = 0
        self._fallbacks = 0
        self._busy_ms = 0.0

    def start(self) -> None:
        with self._lock:
            self._inflight += 1

    def done(self, kind: str, outcome: str, wall_ms: float, fallback: bool) -> None:
        with self._lock:
            self._inflight -= 1
            self._jobs[(kind, outcome)] = self._jobs.get((kind, outcome), 0) + 1
            self._busy_ms += wall_ms
            if fallback:
                self._fallbacks += 1

    def inflight(self) -> int:
        with self._lock:
            return self._inflight

    def render(self, ready: bool) -> str:
        with self._lock:
            lines = [
                "# HELP pyworker_jobs_total Jobs run, by kind and outcome (ok, rejected, failed, timeout, crashed).",
                "# TYPE pyworker_jobs_total counter",
            ]
            for kind in ("docx", "pdf"):
                for outcome in ("ok", "rejected", "failed", "timeout", "crashed"):
                    lines.append(f'pyworker_jobs_total{{kind="{kind}",outcome="{outcome}"}} {self._jobs.get((kind, outcome), 0)}')
            lines += [
                "# TYPE pyworker_inflight gauge",
                f"pyworker_inflight {self._inflight}",
                "# TYPE pyworker_pdf_engine_fallbacks_total counter",
                f"pyworker_pdf_engine_fallbacks_total {self._fallbacks}",
                "# TYPE pyworker_busy_seconds_total counter",
                f"pyworker_busy_seconds_total {self._busy_ms / 1000.0:.3f}",
                "# TYPE pyworker_ready gauge",
                f"pyworker_ready {1 if ready else 0}",
            ]
        return "\n".join(lines) + "\n"


class Service:
    def __init__(self, root: Path, docx_script: Path, pdf_script: Path, concurrency: int) -> None:
        self.root = root.resolve()
        self.docx_script = docx_script
        self.pdf_script = pdf_script
        self.slots = threading.BoundedSemaphore(concurrency)
        self.concurrency = concurrency
        self.metrics = Metrics()
        self.draining = threading.Event()

    # -- engines -----------------------------------------------------------------------------------------------------

    def prince_version(self) -> str | None:
        exe = shutil.which("prince")
        if exe is None:
            return None
        try:
            out = subprocess.run([exe, "--version"], capture_output=True, timeout=20, check=False)
        except (OSError, subprocess.SubprocessError):
            return None
        first = out.stdout.decode("utf-8", errors="replace").splitlines()
        return (first[0] if first else "unknown") if out.returncode == 0 else None

    def weasyprint_ok(self) -> bool:
        try:
            out = subprocess.run([sys.executable, "-I", "-c", "import weasyprint"], capture_output=True, timeout=60, check=False)
        except (OSError, subprocess.SubprocessError):
            return False
        return out.returncode == 0

    def ready(self) -> tuple[bool, Json]:
        scripts = self.docx_script.is_file() and self.pdf_script.is_file()
        root_ok = self.root.is_dir() and os.access(self.root, os.W_OK)
        prince = self.prince_version()
        weasy = self.weasyprint_ok()
        detail: Json = {
            "docx_script": self.docx_script.is_file(),
            "pdf_script": self.pdf_script.is_file(),
            "root": str(self.root),
            "root_writable": root_ok,
            "prince": prince,
            "weasyprint": weasy,
            "concurrency": self.concurrency,
            "inflight": self.metrics.inflight(),
            "draining": self.draining.is_set(),
        }
        return scripts and root_ok and (prince is not None or weasy), detail

    # -- running one job ---------------------------------------------------------------------------------------------

    def _inside_root(self, p: str) -> Path | None:
        path = Path(os.path.realpath(p))
        try:
            path.relative_to(self.root)
        except ValueError:
            return None
        return path if path.is_file() else None

    def _run(self, script: Path, job_file: Path, deadline_s: float, request_id: str, kind: str) -> Json:
        started = time.perf_counter()
        proc = subprocess.Popen(  # noqa: S603 (the argv is fixed; only the job path varies and it is inside the root)
            [sys.executable, "-I", str(script), "--job", str(job_file)],
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            start_new_session=True,
        )
        timed_out = False
        try:
            stdout, stderr = proc.communicate(timeout=deadline_s)
        except subprocess.TimeoutExpired:
            timed_out = True
            self._kill_group(proc.pid)
            stdout, stderr = proc.communicate()
        finally:
            # A grandchild (Prince, a font tool) may still be running after a clean exit of its parent: the group goes too.
            self._kill_group(proc.pid)
        code = proc.returncode
        wall_ms = round((time.perf_counter() - started) * 1000, 2)
        for line in stderr.decode("utf-8", errors="replace").splitlines():
            line = line.strip()
            if line:
                sys.stderr.write(line + "\n")  # the script's own JSON lines already carry the request id
        sys.stderr.flush()
        return {
            "exit": code if code is not None and code >= 0 else None,
            "signal": -code if code is not None and code < 0 else None,
            "timed_out": timed_out,
            "stdout": _tail(stdout),
            "stderr": _tail(stderr),
            "pid": proc.pid,
            "wall_ms": wall_ms,
            "kind": kind,
        }

    @staticmethod
    def _kill_group(pid: int) -> None:
        try:
            os.killpg(pid, signal.SIGKILL)
        except (ProcessLookupError, PermissionError):
            pass

    @staticmethod
    def _outcome(res: Json) -> str:
        if res.get("timed_out") is True:
            return "timeout"
        code = res.get("exit")
        if code == 0:
            return "ok"
        if code == 2:
            return "rejected"
        return "crashed" if code is None else "failed"

    def run_job(self, kind: str, body: Json) -> tuple[int, Json]:
        request_id = str(body.get("request_id", ""))[:64]
        job_file = self._inside_root(str(body.get("job_file", "")))
        if job_file is None:
            return 400, {"error": "job_file is missing, outside PYWORKER_ROOT, or not a file"}
        try:
            deadline_ms = int(str(body.get("deadline_ms", 300000)))
        except ValueError:
            return 400, {"error": "deadline_ms is not an integer"}
        deadline_s = min(max(deadline_ms, 1000), 86_400_000) / 1000.0
        if self.draining.is_set():
            return 503, {"error": "draining", "retryable": True}
        engine = str(body.get("engine", "weasyprint")) if kind == "pdf" else "docx"
        if kind == "pdf" and engine not in ("prince", "weasyprint"):
            return 400, {"error": f"unknown engine {engine!r}"}
        script = self.docx_script if kind == "docx" else self.pdf_script
        waited = time.perf_counter()
        with self.slots:
            queued_ms = round((time.perf_counter() - waited) * 1000, 2)
            self.metrics.start()
            fallback = False
            res: Json = {}
            try:
                _log("info", "job_start", request_id, kind=kind, engine=engine, queued_ms=queued_ms)
                if kind == "pdf" and engine == "prince" and self.prince_version() is None:
                    engine, fallback = "weasyprint", True
                    _log("warn", "engine_fallback", request_id, reason="prince is not installed", engine=engine)
                if kind == "pdf":
                    self._write_engine(job_file, engine)
                res = self._run(script, job_file, deadline_s, request_id, kind)
                if kind == "pdf" and engine == "prince" and self._outcome(res) not in ("ok", "rejected"):
                    # Prince failed or hung on this document: a request must still end with a PDF.
                    first_exit = res.get("exit")
                    _log("warn", "engine_fallback", request_id, reason=f"prince {self._outcome(res)}", exit=first_exit if isinstance(first_exit, int) else None)
                    engine, fallback = "weasyprint", True
                    self._write_engine(job_file, engine)
                    res = self._run(script, job_file, deadline_s, request_id, kind)
                res["engine"] = engine
                res["fallback"] = fallback
                res["queued_ms"] = queued_ms
                return 200, res
            finally:
                outcome = self._outcome(res) if res else "crashed"
                wall = res.get("wall_ms")
                self.metrics.done(kind, outcome, float(wall) if isinstance(wall, (int, float)) else 0.0, fallback)
                _log("info", "job_done", request_id, kind=kind, outcome=outcome, engine=engine, fallback=fallback)

    @staticmethod
    def _write_engine(job_file: Path, engine: str) -> None:
        """The PDF job file carries the engine; rewrite it for a fallback (the file is the caller's scratch)."""
        raw: object = json.loads(job_file.read_text(encoding="utf-8"))
        if isinstance(raw, dict):
            job = {str(k): v for k, v in raw.items()}
            job["engine"] = engine
            job_file.write_text(json.dumps(job), encoding="utf-8")


def make_handler(svc: Service) -> type[BaseHTTPRequestHandler]:
    class Handler(BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"
        server_version = "doc-pyworker"

        def log_message(self, format: str, *args: object) -> None:  # noqa: A002 (the stdlib's signature)
            return  # one structured line per job is written by the service; probes would only add noise

        def _send(self, status: int, payload: Json | str, content_type: str = "application/json") -> None:
            data = (payload if isinstance(payload, str) else json.dumps(payload)).encode("utf-8")
            self.send_response(status)
            self.send_header("Content-Type", content_type)
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)

        def do_GET(self) -> None:  # noqa: N802 (the stdlib's name)
            if self.path == "/healthz":
                self._send(200, {"status": "ok"})
            elif self.path == "/readyz":
                ok, detail = svc.ready()
                self._send(200 if ok else 503, {"ready": ok, **detail})
            elif self.path == "/metrics":
                ok, _ = svc.ready()
                self._send(200, svc.metrics.render(ok), "text/plain; version=0.0.4")
            else:
                self._send(404, {"error": "not found"})

        def do_POST(self) -> None:  # noqa: N802
            kind = {"/v1/docx": "docx", "/v1/pdf": "pdf"}.get(self.path)
            if kind is None:
                self._send(404, {"error": "not found"})
                return
            try:
                length = int(self.headers.get("Content-Length", "0"))
            except ValueError:
                length = -1
            if length <= 0 or length > MAX_BODY:
                self._send(400, {"error": "a JSON body of at most 64 KiB is required"})
                return
            try:
                raw: object = json.loads(self.rfile.read(length))
            except ValueError:
                self._send(400, {"error": "the body is not JSON"})
                return
            if not isinstance(raw, dict):
                self._send(400, {"error": "the body must be a JSON object"})
                return
            try:
                status, payload = svc.run_job(kind, {str(k): v for k, v in raw.items()})
            except Exception as exc:  # noqa: BLE001 - a bug here must answer 500, not drop the connection
                _log("error", "internal_error", error=f"{type(exc).__name__}: {exc}")
                status, payload = 500, {"error": "internal error", "retryable": True}
            self._send(status, payload)

    return Handler


def serve(argv: list[str]) -> int:
    host = os.environ.get("PYWORKER_HOST", "0.0.0.0")  # noqa: S104 (a container service on a private network)
    port = int(os.environ.get("PYWORKER_PORT", "9100"))
    concurrency = max(1, int(os.environ.get("PYWORKER_CONCURRENCY", "2")))
    root = Path(os.environ.get("PYWORKER_ROOT", "/app/document_output"))
    docx_script = Path(os.environ.get("PYWORKER_DOCX_SCRIPT", str(HERE / "docx_worker.py")))
    pdf_script = Path(os.environ.get("PYWORKER_PDF_SCRIPT", str(HERE / "pdf_worker.py")))
    drain_s = float(os.environ.get("PYWORKER_DRAIN_SECONDS", "120"))
    if "--check" in argv:
        svc = Service(root, docx_script, pdf_script, concurrency)
        ok, detail = svc.ready()
        print(json.dumps({"ready": ok, **detail}))
        return 0 if ok else 1
    svc = Service(root, docx_script, pdf_script, concurrency)
    server = ThreadingHTTPServer((host, port), make_handler(svc))
    server.daemon_threads = True
    stop = threading.Event()

    def on_term(signum: int, _frame: object) -> None:
        _log("info", "draining", signal=signum, inflight=svc.metrics.inflight())
        svc.draining.set()
        stop.set()

    signal.signal(signal.SIGTERM, on_term)
    signal.signal(signal.SIGINT, on_term)
    thread = threading.Thread(target=server.serve_forever, kwargs={"poll_interval": 0.2}, daemon=True)
    thread.start()
    ok, detail = svc.ready()
    _log("info", "listening", host=host, port=port, concurrency=concurrency, ready=ok, prince=str(detail.get("prince")), weasyprint=bool(detail.get("weasyprint")))
    stop.wait()
    deadline = time.monotonic() + drain_s
    while svc.metrics.inflight() > 0 and time.monotonic() < deadline:
        time.sleep(0.1)
    left = svc.metrics.inflight()
    server.shutdown()
    server.server_close()
    _log("info" if left == 0 else "warn", "stopped", unfinished=left)
    return 0


if __name__ == "__main__":
    raise SystemExit(serve(sys.argv[1:]))
