# P7 plan: the TypeScript build as a full end-to-end DOC replacement (task doc-full-ts)

Written first (milestone M0), before any P7 code. Evidence tags used everywhere in this directory: **RAN** (executed, output kept), **READ** (read in code or docs), **INFERRED** (reasoning only). DOC base commit `50de21cd3bab5c6f48961da64257118f6f0ead0e` (fork `linearcombination-ai/DOC`, branch `doc-dev.walink.org`). This plan is updated in place when a risk turns out different; every change is dated at the bottom.

## 1. Goal and the bar

The captain wants to *run and try* this build as a complete system: `docker compose up` on their own machine, open one URL, use DOC's own UI, get documents. The bar for "replacement" is **DOC's own tests**: its `tests/e2e/doc` files pass against this stack in GitHub Actions on the fork, next to the same tests on DOC's FastAPI stack in the same run, and DOC's Playwright frontend tests run against this stack. No pull request, no merge, no registry push; images are built on the runner only.

Standing rules this plan keeps (from the brief and `doc-architecture-decisions.md`): core is Rust (compiled to wasm, called from the TypeScript shell); docx and PDF stay Python behind a seam (Prince preferred, WeasyPrint fallback); DOC is production, so a request must end with a document (retry, self-heal, generous ceilings, clear logging, never a bare error on a transient condition); names kept as written with findings returned; no `typing.Any`; STET and Passages are **out of this phase** (their e2e tests and routes are listed as excluded, not hidden); the captain does the visual checks in Word and the acceptance.

## 2. The system in containers

```
 browser --8001--> frontend  (DOC's own image: nginx + SvelteKit node server; only environment changed)
 browser --5005--> gateway --> api      (node src/main.ts: Hono, light wasm: keys, book lists, status polling, enqueue)
 browser --8089--> gateway --> output volume (autoindex, read only: <key>.docx|html|pdf, DOC's fileserver behaviour)
                              api --> valkey (BullMQ queue, status strings, metrics, clone locks, email records)
                              worker (node src/worker-main.ts + full wasm core on a worker thread)
                                 |--> git clones into the assets volume (cross-process clone lock)
                                 |--> supervised Python pool (docx, html finish, PDF via Prince, WeasyPrint fallback)  [same container]
                              output volume (rw: worker, ro: gateway), assets volume (worker), valkey data volume (AOF)
```

* **api** image (`docker/Dockerfile.shell`, target `api`): multi-stage; Rust stage builds the wasm modules, node stage installs production dependencies, final stage is `node:26-bookworm-slim` with no Python, no git, no compiler, non-root, `HEALTHCHECK` on `/health/status`.
* **worker** image (`docker/Dockerfile.worker`): base `python:3.13-slim-bookworm` as DOC's own Dockerfile (so the Prince `.deb` and the font packages install the same way), with the node binary copied from the official node image, the wasm modules and production `node_modules` copied from the shell build stages, non-root, a health check that asks the worker's own `/health` (a heartbeat port; added in M1) . **Decision, recorded**: the Python docx/PDF pool stays as child processes of the node worker inside this one container (the P6 design, tested: crash, hang, memory ceiling, recycle). A separate Python container behind a network seam would lose that process supervision and is listed as a later option with its cost (INFERRED), not built.
* **gateway**: `nginx:alpine` with DOC's `web/default.conf` (the fileserver autoindex) and the API reverse proxy on the same ports DOC publishes (5005 API, 8089 files), `X-Request-Id` generation, CORS headers as DOC's `BACKEND_CORS_ORIGINS` gives, body-size limit, `Cache-Control` and `Content-Disposition` on documents, `/gateway-health`.
* **frontend**: DOC's `frontend/Dockerfile` **unchanged**; configuration only through `PUBLIC_BACKEND_API_URL`, `PUBLIC_FILE_SERVER_URL`, `PUBLIC_LOGROCKET_ID` (runtime, `$env/dynamic/public`), as DOC's own compose does.
* **valkey** (`valkey/valkey:9`, BSD-3) with AOF `everysec`, `noeviction`, a named volume; a one-line override switches to Redis 8 (RSALv2/SSPLv1/AGPLv3 at the operator's choice). Default Valkey because the licence is permissive and the whole suite passes on both (P6).
* No .NET USFM sidecar: USFM is parsed in the Rust core (kitchen's parser). Flower is not needed (the admin tool and metrics replace it).

Compose files (all under `platform-ts/`, DOC's own compose files untouched): `docker-compose.yml` (the full system, with named volumes, resource limits, restart policies, health-gated `depends_on`), `docker-compose.e2e.yml` (override for CI: the test runner, fixed ports, no restart loops, a stats sampler).

## 3. Milestones, in order, each with the test that proves it

| M | Scope | Proof (what must be true, where it runs) |
|---|---|---|
| **M0** | This plan; `checkpoint.md`; branch `exp/platform-ts` from `doc-dev.walink.org` | Plan committed first; branch exists locally and is pushed only after M1 validates |
| **M1** | Container files: `Dockerfile.shell`, `Dockerfile.worker`, gateway, frontend config, valkey config, `docker-compose.yml`, e2e override, workflow `platform-ts.yml`, DOC's `docker-publish.yml` disabled on the branch, README, RUNBOOK for containers | **No container engine here.** hadolint clean on every Dockerfile; compose files validate against the compose-spec JSON schema; `actionlint` clean on the workflow; the build scripts the Dockerfiles call are run on the host (wasm build, `npm ci --omit=dev`, python deps); the **whole stack started as plain processes** (valkey, api, worker, a static file server standing in for the gateway) passes a smoke test against real network and returns a docx and an html |
| **M2** | First green CI | Run URL: images build on the runner (times and sizes recorded), `docker compose up --wait` makes every service healthy, a smoke script (health via gateway, a Jude docx and html through :5005, download through :8089, UI on :8001) passes; logs and timings uploaded |
| **M3** | Parity with DOC for every request shape the UI and API accept | (a) per-chapter and per-book `reg`, Writer repositories, resources absent from the Primary catalog resolved as DOC resolves them (READ DOC's `resource_lookup.py` and `parsing.py` first), TN chapter intro files tolerated; HTML in DOC's markup (`class="versemarker"` etc.) plus DOC's CSS; docx; PDF through Prince; 30-minute cache; email; the status strings and result shapes the frontend reads. (b) **DOC's `tests/e2e/doc` files run in CI against this stack and against DOC's FastAPI stack in the same run; pass table per file**. Local loop first: the same tests against the plain-process stack on this host with the P6 shim |
| **M4** | Frontend end to end | DOC's Playwright tests (`frontend/tests/e2e/doc_test_*.ts`; STET and Passages files excluded and listed) against this stack in CI, same tests on DOC's own stack for the baseline where affordable; every difference classified (our bug, DOC baseline flake, network, test assumption) |
| **M5** | Operations in containers | resource limits in compose, memory and CPU per container measured in CI (`docker stats` sampled at rest, during e2e, during a burst), SIGTERM drain test, `docker kill` of the worker mid-job (stalled recovery), restart policy test, Valkey restart with AOF, soak (as long as CI minutes allow), rollback story written and exercised (previous image tag, same volumes) |
| **M6** | Report and handover | `report.md`, `COMPARISON.md` (P6 headings plus Docker and CI, DOC e2e pass table, Memory per container), `RUNBOOK.md`, `README.md`, source copy under `data/doc-full-ts/prototype/`; what the captain should compare; what is not done |

## 4. CI plan (workflow `platform-ts`, file `.github/workflows/platform-ts.yml`)

Triggers: `push` on `exp/platform-ts` and `exp/platform-ts/**`, and `workflow_dispatch` (with an input that picks which jobs to run, so a fix to the e2e job does not rebuild everything). `concurrency` cancels superseded runs on the same ref. Nothing is pushed to a registry; there are no secrets.

| Job | What it does | Artifacts |
|---|---|---|
| `static` | hadolint (every Dockerfile), `docker compose config` for both compose files, actionlint, `tsc --noEmit`, mypy strict on all Python, no-`any` and no-`typing.Any` greps | lint log |
| `unit` | build wasm (cargo cache), `npm ci`, the whole vitest suite against **Valkey 9.2** (service container) , then the Redis 8 run when affordable | vitest results, timings |
| `build-<image>` (matrix: shell/api, worker, frontend, gateway, doc-baseline) | `docker/build-push-action` with `outputs: type=docker,dest=...tar`, GHA cache scope per image; records build seconds and `docker image ls` size | image tar (1-day retention), `build-times.json` |
| `smoke` | load images, `docker compose up -d --wait`, smoke script, `docker stats` once, `docker compose logs`, `down -v` | logs, smoke output, stats |
| `e2e-api-ts` (matrix: 3 groups like DOC's CI) | stack up, DOC's `tests/e2e/doc` (the same marker expression DOC's CI uses: `not all and not randomized and not english_randomized`) through the test runner (the DOC image plus the shim `ci/doc_e2e_shim.py`), junit xml, a stats sampler every 5 s | junit, stats csv, logs |
| `e2e-api-doc` (same matrix) | DOC's own stack (DOC's `docker-compose.yml` + `docker-compose.api-test.yml`, its own Dockerfile minus the Calibre step that upstream PR 415 removes) and the same tests unchanged | junit, stats csv, logs |
| `e2e-ui-ts` (matrix: shards) | stack up, DOC's Playwright image and tests with `network_mode: host`, ports 8001/5005/8089 | playwright report, junit |
| `e2e-ui-doc` | the same tests on DOC's stack (baseline; run once, not sharded, if minutes allow) | report |
| `ops` | M5 scenarios against the compose stack: SIGTERM drain, `kill -9` of the worker, Valkey restart, burst, soak, restart policy, rollback | per-scenario logs and csv |
| `report` | collects junit, timings and stats into one markdown summary (`$GITHUB_STEP_SUMMARY`) and a downloadable `p7-summary` artifact | summary |

Run-time discipline: each run is minutes of the captain's allowance. Validate locally first; batch fixes; one push per hypothesis; `workflow_dispatch` with a narrow job list for e2e-only iterations; every run's URL, duration and what it proved is written to `checkpoint.md`. Inspect with `gh-axi` (never `gh` directly for write operations).

## 5. Local validation without a container engine (what "validated" means before a push)

1. `hadolint` (static binary, downloaded to `scratch/`), every Dockerfile.
2. compose-spec JSON schema validation of both compose files (`check-jsonschema`), plus a script that checks every `depends_on` target exists, every `${VAR}` has a default or is documented, ports do not collide.
3. `actionlint` on the workflow.
4. Every `RUN` step that is a script (wasm build, `npm ci --omit=dev`, python requirements install, kitchen fetch) is executed on the host in a clean directory laid out like the image.
5. The full stack as plain processes: valkey, api, worker, nginx replaced by a tiny static server for the output directory; the smoke script and the DOC e2e shim run against it on this host with the real network.
6. Things that cannot be checked here (stated, not guessed): the image builds themselves (apt, Prince `.deb`, fonts), the gateway's nginx syntax (checked by `RUN nginx -t` inside the image build), health checks inside containers, memory limits, `docker stats`.

## 6. Risks and how each is handled

| # | Risk | Handling |
|---|---|---|
| 1 | **Parity work is large** (3 known gaps from P6: non-Primary catalog resources, per-chapter/per-book Writer and `reg` repositories, TN chapter-intro tolerance; plus DOC's HTML markup and CSS; plus the other request shapes the e2e files use) | Read DOC's resolver first; implement in the locator/parse stage of the core (pure, testable) with golden fixtures taken from DOC's own behaviour; the local loop with DOC's tests; classify every failing test |
| 2 | DOC's tests hit the **real network** (catalog GraphQL, content git hosts): flakes and rate limits on CI | Same tests, same run, both stacks, so a flake shows on both; retries inside the service (catalog, git); record per-test durations; rerun a failed group once and say so |
| 3 | Image builds are slow or fail only in CI (apt packages, Prince `.deb` URL, fonts git clone, node 26 image tags) | Copy DOC's own install steps; pin versions I could look up (Docker Hub tags checked over HTTP, not with docker); one build job per image so a failure is isolated; GHA cache |
| 4 | Prince in CI: licence and watermark | Installed exactly as DOC's Dockerfile does; the seam picks Prince when the binary exists and the request says `use_prince` (or the configured default), else WeasyPrint and a finding; the licence situation is described, not decided, in COMPARISON |
| 5 | Memory: the worker image carries node + python + wasm + fonts; WeasyPrint on whole books is large | Per-format memory ceilings (P6), compose `mem_limit`, per-container measurements; Prince runs as an external process with its own ceiling |
| 6 | The host has 4 CPUs and 11 GB shared with the other build's worker | `CARGO_BUILD_JOBS=2`, one heavy job at a time, a status line before anything over 3 GB, no docker here, everything under `scratch/` |
| 7 | DOC's e2e tests use FastAPI's in-process `TestClient` and `settings.DOCUMENT_OUTPUT_DIR` | The P6 shim (an httpx client on a URL, mypy strict) plus the shared output volume mounted at `/app/document_output` in the test runner |
| 8 | The DOC baseline stack may itself be red in CI (the Calibre download failure seen on 2026-11-24, no Prince) | Baseline image built from DOC's Dockerfile with the Calibre step and the mypy steps removed (what upstream PR 415 does to Calibre), stated in COMPARISON; baseline failures are classified, not hidden |
| 9 | Playwright tests wait on the live catalog for tens of minutes in places (timeouts up to 8,200,000 ms in the source) | The config's own cap (15 min global in CI) governs; failures classified as environment when the baseline fails the same way |

## 7. Cross-learning

At every milestone read `data/doc-full-rs/checkpoint.md` (read only) and record in the report what was taken. If it is absent, say so.

## 8. Change log

* 2026-11-25 M0: first version.
* 2026-11-26 M2/M3 (after the host reboot, relaunch): the plan held; deviations, each found by a CI run, are in section 9. Status of every milestone is in section 10.

## 9. What changed from this plan, and why (all found by running, not by reading)

| # | Plan said | What happened | Evidence |
|---|---|---|---|
| 1 | api and worker images run as different non-root users | Two images writing the same named volumes raced on first mount and one lost (`EACCES`); then a read-only secondary mount copied another image's ownership into the empty volume. Fixed with one uid (1000), a `volume-init` one-shot and `nocopy: true` on every secondary mount | CI runs 2 and 5 |
| 2 | `docker compose up --wait` | Replaced by `ci/wait-healthy.sh` (a `--wait` over a one-shot service that exits 0 behaves differently across compose versions) | `ci/wait-healthy.sh` |
| 3 | api image has no git | It has git: the lookups that read repositories (`book_codes_for_lang_from_usfm_only`, `chapters_in_books`) clone them, as DOC's API does | `docker/Dockerfile.shell` |
| 4 | e2e jobs run on every push | Too slow for every push: the plan job reads markers in the head commit message (`[e2e]`, `[e2e-ui]`, `[ops]`, `[all]`); `workflow_dispatch` only works for a workflow file on the repository's default branch, so it cannot be used from this branch | `.github/workflows/platform-ts.yml` job `plan` |
| 5 | HTML in DOC's markup was one of three known gaps | It was six: DOC's header templates and CSS (jinja2 in the worker), `hr` separators, the title page strings (`get_languages_title_page_strings`), localized book names (USFM markers, manifest, `front/title.txt`, Writer repositories stitched first), an empty document when nothing resolves, and **DOC's on-the-fly USFM repairs for 98 languages** (`usfm_error_detection_and_fixes.py`, ported with `fancy-regex`) | CI run 6 (first tables), run 7 (all green) |
| 6 | DOC's baseline from DOC's Dockerfile minus Calibre | As planned (`ci/make-baseline-dockerfile.sh`); it also served as the test-runner image for both stacks | CI run 4 onward |
| 7 | the test runner reads the output volume | Same, read only and `nocopy` | `docker-compose.e2e.yml` |
| 8 | Prince would be checked | Smoke test now makes a PDF with `use_prince: true` and fails if a `PdfEngineFallback` finding exists | `ci/smoke.sh`, run 6 and 7 smoke |
| 9 | a missing book is a permanent error | DOC builds an empty book; so the pipeline does, with a finding. Only a request where **every** requested book is absent from its repository is a permanent `resource_unavailable` (no content at all) | `src/jobs/pipeline.ts`, `test/job.fault.test.ts` |
| 10 | on the last attempt a repository that cannot be fetched is an error | The last attempt builds the document from the repositories it has and returns a `RepoNotFetched` finding for each missing one (a request ends with a document); if none could be fetched it is still the transient error | `src/jobs/pipeline.ts` |

## 10. Milestone status

| M | Status | Proof |
|---|---|---|
| M0 | done | this file, committed first |
| M1 | done | container files, compose, workflow; hadolint, compose-spec schema, actionlint clean; stack as plain processes |
| M2 | done | CI run 3: images build, stack healthy, smoke passes |
| M3 | done | CI run 7: DOC's `tests/e2e/doc` 334 passed / 0 failed / 124 skipped on this stack and on DOC's FastAPI baseline, 0 tests differ |
| M4 | done | CI run 7: DOC's Playwright `doc_test_*.ts` 18 / 0 / 10 on both stacks |
| M5 | see COMPARISON.md "Docker and CI" and the ops results | CI `ops` job |
| M6 | report, handover | `report.md` |
