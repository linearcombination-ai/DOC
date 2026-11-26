# DOC platform experiment, P7: the Rust build as a full system

An experiment, not a pull request: nothing here replaces DOC's code. DOC's own backend, frontend and compose files in the repository root are untouched; this directory is a second, complete backend (a Rust shell and core with a job runner, and a Python service for docx and PDF) that runs **instead of** DOC's FastAPI + Celery + .NET sidecar in its own compose file, behind DOC's own UI. The other build (a TypeScript shell around the same Rust core) lives on branch `exp/platform-ts`; the shell choice stays open until you have compared both.

## Run it (shortest path)

```bash
git fetch origin exp/platform-rs && git switch exp/platform-rs     # the fork linearcombination-ai/DOC; nothing is in upstream
cd platform-rs
docker compose up --build        # first build: about 10 to 20 minutes (Rust, fonts, Prince, DOC's UI); later builds use the layer cache
```

Open **http://localhost:8001** (DOC's own UI, unchanged). The services answer on DOC's own ports: UI `:8001`, API `:5005`, generated files `:8089`.

Things to try, in this order:

1. **Make a document.** English, Jude (a short book), Unlocked Literal Bible + Translation Notes, *Generate File*. Watch the status text change ("Locating assets", "Provisioning asset files", "Parsing asset files"...). The link appears when it is done; the same file is also listed at http://localhost:8089 .
2. **Make the same document again** (a cache hit: it answers at once from the 30-minute cache), then change one option (a new document).
3. **PDF.** Choose PDF: the Python service uses Prince when the package for your CPU architecture installed (`docker compose logs pyworker` says which engine ran), otherwise WeasyPrint, and the document's findings file says so. Prince's non-commercial licence puts its notice on the first page, exactly as in DOC's own image today.
4. **A bigger request**: two languages, by chapter, with Translation Words. The first request clones the repositories; later ones use the cache.
5. **Break it on purpose** (the point of the build): `docker compose kill -s KILL worker` while a document is in progress, then `docker compose start worker`; the job is picked up again and the document still arrives (a crash *inside* the container is restarted by the engine's policy by itself; `docker kill` counts as a manual stop). `docker compose stop worker` drains the running job first. `docker compose restart valkey`: the stack reconnects, queued work survives (append-only file).
6. **Look inside**: `curl localhost:5005/health/status`, `curl localhost:5005/metrics` (Prometheus text), `docker compose logs -f worker` (one JSON line per stage and attempt, with the request id the UI call carried), `docker stats`, `curl localhost:8089/<key>.findings.json` (what was left out, repaired or substituted, in words, next to each document).

Stop: `docker compose down` (keep documents, clones and queue) or `docker compose down -v` (remove them too).

Optional settings (environment or a `.env` file next to `docker-compose.yml`, see `.env.example`): `DATA_API_URL`, `ASSET_CACHING_PERIOD`, `SEND_EMAIL` with `SMTP_HOST`, `SMTP_PORT`, `SMTP_USER`, `SMTP_PASSWORD`, `FROM_EMAIL_ADDRESS`, `API_HOST_PORT`, `FRONTEND_HOST_PORT`, `FILESERVER_HOST_PORT`, `WORKER_MEM`, `WORKER_CONCURRENCY`, `PYWORKER_MEM`, `PYWORKER_CONCURRENCY`. Every setting and what it does: `docs/RUNBOOK.md`.

## What is where

| Path | What |
|---|---|
| `docker-compose.yml` | the full system (valkey, pyworker, api, worker, gateway, frontend); limits, restart policies, health checks, stop grace periods |
| `docker-compose.e2e.yml` | override for the CI end-to-end runs (DOC's own tests in DOC's own image, pointed at the gateway) |
| `containers/` | `core.Dockerfile` (the Rust shell, core and job runner: one image, roles `api` and `worker`), `pyworker.Dockerfile` (Python docx and PDF service, Prince and WeasyPrint, fonts), `gateway/` (nginx), `valkey.conf` |
| `crates/` | the Rust workspace: contract, core, adapters, job runner, the `doc-jobd` binary |
| `python/` | the docx and PDF side of the seam (`seam_server.py` is the HTTP service the worker calls) |
| `ci/` | scripts used by `.github/workflows/platform-rs.yml`: wait for health, smoke test, operations scenarios, load generator, pass table, stats sampler |
| `e2e/`, `scripts/` | the pytest plugin that points DOC's tests at a URL, and the host-side scripts (the stack as plain processes, benchmarks) |
| `docs/P7-PLAN.md` | the plan, written first |
| `docs/RUNBOOK.md` | operating the containers |
| `docs/COMPARISON.md` | evidence tables, same headings as the TypeScript build's file |
| `README.p6.md` | the P6 readme (running the shell without containers) |

## Evidence

CI runs on the fork (`linearcombination-ai/DOC`, workflow `platform-rs`, branch `exp/platform-rs`): images are built on the runner (never pushed), the stack is brought up with `docker compose`, and DOC's own tests run against it next to the same tests on DOC's FastAPI + Celery stack, in the same run. The results are filled in at the end of the work, in `docs/COMPARISON.md` and `../report.md`.

## What it does not do yet

STET and Passages routes and tests are out of this phase (listed as excluded in the comparison). Word was not opened on any output: your visual check of the docx in Word is the open item. Prince runs in its non-commercial mode (as DOC's image does).
