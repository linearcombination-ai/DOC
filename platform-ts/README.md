# DOC platform experiment, P7: the TypeScript build as a full system

An experiment, not a pull request: nothing here replaces DOC's code. DOC's own backend, frontend and compose files in the repository root are untouched; this directory is a second, complete backend (Rust core compiled to wasm, TypeScript shell, Python docx/PDF seam) that runs **instead of** DOC's FastAPI + Celery in its own compose file, behind DOC's own UI. The other build (Rust shell) lives on branch `exp/platform-rs`; the shell choice stays open until you have compared both.

## Run it (shortest path)

```bash
cd platform-ts
docker compose up --build        # first build: roughly 10 to 20 minutes (Rust core, node, fonts, Prince, DOC's UI); later builds use the layer cache
```

Open **http://localhost:8001** (DOC's own UI). The services answer on DOC's own ports: UI `:8001`, API `:5005`, generated files `:8089`.

Things to try, in this order:

1. **Make a document.** English, Jude (a short book), Unlocked Literal Bible + Translation Notes, *Generate File*. Watch the status text change ("Receiving request", "Locating assets", "Assembling content", "Converting to Docx"...). The link appears when it is done; the same file is listed at http://localhost:8089 .
2. **Make the same document again** (cache hit in about 50 ms), then change one option (a new document).
3. **PDF.** Choose PDF: the worker uses Prince when the Prince package installed for your CPU architecture (the log says which engine ran), otherwise WeasyPrint. Prince's non-commercial licence puts its notice on the first page, as in DOC today.
4. **A bigger request**: two languages, by chapter, with Translation Words (a first request clones the repositories; later ones use the 30-minute cache).
5. **Break it on purpose** (the point of the build): `docker compose kill -s KILL worker` while a document is in progress. The job is picked up again after the lock expires (about 30 seconds) and the document still arrives. `docker compose stop worker` instead drains the running job first. `docker compose restart valkey` while idle: the stack reconnects by itself.
6. **Look inside**: `curl localhost:5005/health/status`, `curl localhost:5005/metrics` (Prometheus text), `docker compose logs -f worker` (one JSON line per stage and attempt, with the request id the UI call carried), `docker stats`.

Stop: `docker compose down` (keep documents, clones and queue) or `docker compose down -v` (remove them too).

Optional settings (environment or a `.env` file next to `docker-compose.yml`): `PDF_ENGINE` (`auto` default, `prince`, `weasyprint`, `off`), `DATA_API_URL`, `VALKEY_PASSWORD`, `SEND_EMAIL` with `SMTP_HOST`/`SMTP_PORT`/`SMTP_USER`/`SMTP_PASSWORD`/`FROM_EMAIL_ADDRESS`, `API_HOST_PORT`, `FRONTEND_HOST_PORT`, `FILESERVER_HOST_PORT`, `WORKER_MEM_LIMIT`, `WORKER_CONCURRENCY`. Every setting and what it does: `docs/RUNBOOK.md`.

## What is where

| Path | What |
|---|---|
| `docker-compose.yml` | the full system (valkey, api, worker, gateway, frontend); limits, restart policies, health checks, stop grace periods |
| `docker-compose.e2e.yml` | override for the CI end-to-end runs (the test runner) |
| `docker/` | `Dockerfile.shell` (api), `Dockerfile.worker` (worker: node + Python + Prince + fonts), `gateway/` (nginx), `valkey.conf` |
| `core/` | the Rust core (workspace), built to wasm |
| `src/` | the TypeScript shell: API, job runner, acquirer, catalog client, observability |
| `python/` | the docx / HTML / PDF seam (supervised Python processes) |
| `ci/` | smoke test, DOC's end-to-end test shim, stats sampler, summary scripts used by `.github/workflows/platform-ts.yml` |
| `test/`, `tools/`, `scripts/` | tests (real Valkey/Redis, git, SMTP, Python, wasm), soak and bench tools, build scripts |
| `docs/P7-PLAN.md` | the plan, written first |
| `docs/RUNBOOK.md` | operating the containers |
| `docs/COMPARISON.md` | evidence tables, same headings as the Rust build's file |
| `docs/LOCAL-STACK.md` | running the same stack as plain processes (no docker), for development |

## Evidence

CI runs on the fork (`linearcombination-ai/DOC`, workflow `platform-ts`): images are built on the runner (never pushed), the stack is brought up with `docker compose`, DOC's own `tests/e2e/doc` and Playwright tests run against it next to the same tests on DOC's FastAPI stack. Run URLs, timings and pass tables are in `docs/COMPARISON.md`.

## What it does not do yet

STET and Passages routes and tests are out of this phase (listed as excluded in the comparison). Word was not opened on any output (your check). See the report for the full list of what was and was not measured.
