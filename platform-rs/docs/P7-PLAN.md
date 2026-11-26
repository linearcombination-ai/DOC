# P7 plan: the Rust build as a full end-to-end DOC backend replacement (task `doc-full-rs`)

Written first (M0), before any code of this phase. Evidence tags used in this file and in the other documents: **RAN** (executed, output kept),
**READ** (source or document read), **INFERRED** (reasoning only). DOC base commit `50de21cd3bab5c6f48961da64257118f6f0ead0e`
(the fork's `doc-dev.walink.org`). Branch `exp/platform-rs` on the fork `linearcombination-ai/DOC` only. Nothing merged, no pull request.

## 1. Goal and the bar

The captain can run `docker compose up` on their own machine, open one URL, and use DOC's real UI against a backend that is a Rust
core and shell with docx/PDF in a Python worker behind a seam. "Proved" means: **DOC's own tests, unmodified, run in GitHub Actions
on the fork against this stack, with a per-test-file pass table next to the same tests on DOC's FastAPI stack in the same CI run.**
The new look stays an option; the default HTML is DOC-markup-compatible so DOC's e2e tests can parse it.

Standing rules carried (READ, `data/doc-architecture-decisions.md`): core is Rust; docx and PDF stay Python behind a seam (Prince
preferred, WeasyPrint fallback); a request must end with a document (retry, self-heal, generous ceilings, clear logging, no bare
error on a transient condition); names kept as written with findings returned; no `typing.Any` in Python; no merge, no pull request;
the captain does the Word checks and the acceptance. STET and Passages are later (their tests are excluded and listed).

## 2. Architecture of the container stack

```
 browser ─┬─ :8001  frontend   DOC's own SvelteKit image (frontend/Dockerfile unchanged; PUBLIC_* env only)
          ├─ :5005  gateway ── nginx (DOC's nginx idea, adapted) ──► api:8081   (doc-jobd role=api)
          └─ :8089  gateway ── same container, second server block ──► /app/document_output (read-only, DOC's file URLs)
 api ──► valkey ◄── worker (doc-jobd role=worker: locate → git clones → Rust core → HTTP seam → place file → email)
                         └──► pyworker:9100  Python docx + PDF service (Prince, WeasyPrint fallback, fonts), killable per job
 volumes: output (/app/document_output, shared by api, worker, pyworker, gateway, test runner), assets (git clones), scratch
```

* **Ports and URLs are DOC's**: 5005 API, 8089 files, 8001 UI, so the unchanged frontend and DOC's tests work.
* **Seam across containers (decision, P7-D1):** the P6 seam was a child process; here it is an HTTP call from the worker to the
  Python service (`POST /v1/docx`, `POST /v1/pdf`), job files exchanged on the shared volume, deadline enforced by the service
  (subprocess in its own process group, killed at the deadline) and by the client; failures map to the existing transient/permanent
  codes. The child-process modes stay as fallbacks (`DOC_DOCX_MODE=per_job|persistent|remote`); containers use `remote`.
* **Redis or Valkey:** Valkey 9.2.0 (BSD-3; P6 passed the full suite on it) with AOF `everysec`, `noeviction`, volume-backed.
* **One Rust image, two roles.** `doc-jobd` with `DOC_JOBD_ROLE=api` and `=worker` (same binary, same image, scaled separately).
* **Prince**: installed in the Python worker image exactly as DOC's Dockerfile does (`prince_16.1-1_debian12_amd64.deb`),
  same base (`python:3.13-slim-bookworm`) and fonts; WeasyPrint is the fallback when Prince is missing or fails. Licence stays
  as in the P6 comparison (the captain handles it; the unregistered Prince adds its notice to PDFs).

## 3. Milestones, ordered, each with the test that proves it

| # | Milestone | Proved by (where) |
|---|---|---|
| M0 | This plan, branch `exp/platform-rs`, workspace in `platform-rs/`, checkpoint | file exists, first commit (local) |
| M1 | Container files: core, pyworker (+ HTTP seam in Rust and Python), gateway, valkey config, `docker-compose.yml`, e2e override; the workflow file | locally, no container engine: hadolint clean on every Dockerfile, compose files validate against the compose-spec JSON schema, `nginx -t` on the gateway config, `actionlint` on the workflow, `cargo test`, `mypy --strict`, **the whole stack as plain processes** (valkey, api, worker, python service, nginx gateway, the built frontend) answers a request end to end through the gateway |
| M2 | First green CI: every image builds in Actions, the stack comes up, health checks pass through the gateway and the UI loads | Actions run URL, job `smoke` green, `docker compose ps` all healthy, image sizes and build times recorded |
| M3 | Parity: every request shape the UI/API accept (per-chapter and per-book `reg` incl. Writer repos; resources outside the Primary catalog resolved as DOC does (`add_data_not_supplied_by_data_api`, non-primary lookups); TN chapter intros missing tolerated; status strings and result shapes the UI reads; 30-minute cache; email; DOC-markup HTML; docx; PDF through Prince in CI) | DOC's `tests/e2e/doc` files run in Actions against this stack and against DOC's FastAPI stack in the same run; per-file pass table; locally, the same tests against the plain-process stack for the fast loop |
| M4 | The frontend end to end | DOC's Playwright tests (`frontend/tests/e2e/doc_test_*.ts`) against this stack in Actions (and against DOC's stack for classification); differences classified |
| M5 | Operations in containers: P6 fault, soak and metrics work carried over where CI can do it; resource limits in compose; memory/CPU per container measured in CI; graceful shutdown on SIGTERM; restart policy; rollback story | CI job `ops`: kill-and-recover (worker, pyworker, valkey), SIGTERM drain, `docker stats` samples, short soak; numbers in COMPARISON |
| M6 | Report and handover | `data/doc-full-rs/report.md`, README, RUNBOOK (container deployment), COMPARISON (P6 headings plus the three new), source copy |

## 4. The CI plan (workflow `.github/workflows/platform-rs.yml`, name "Platform RS")

Triggers: `push` on `exp/platform-rs` and `exp/platform-rs/**`, and `workflow_dispatch` (inputs choose which job groups to run, so a
fix does not rerun everything). DOC's own `docker-publish.yml` is switched to `workflow_dispatch` only on this branch (it pushes images
and is red by design on the fork). **No registry push, no secrets**: images are built into the runner (`type=docker,dest=tar` plus the
Actions cache for layers, as DOC's own workflow does), passed between jobs as artifacts.

| Job | What | Artifacts |
|---|---|---|
| `rust` | fmt, clippy, `cargo test --workspace` against a real Valkey/Redis, `mypy --strict` over `platform-rs/python` and scripts, hadolint, compose schema check, `nginx -t` | test logs, timings |
| `images` (matrix: core, pyworker, frontend, gateway, doc-api, doc-usfmparser, doc-fileserver, doc-frontend-tests) | build each once; record size and wall time; save as tar | image tars (1 day), `images.json` |
| `smoke` | load images, `compose up`, wait healthy, `curl` through the gateway (health, `chapters_in_books`, one document job), UI loads, `docker stats` | logs, `docker-stats.csv` |
| `e2e-rs` (3 groups, same split as DOC's CI) | DOC's tests in a test-runner container (DOC's api image, unmodified tests, `-p e2e_remote` pointing at the gateway), marker expression `not all and not randomized and not english_randomized`, STET and Passages excluded | junit XML, logs |
| `e2e-doc` (3 groups) | the identical tests against DOC's FastAPI + Celery + sidecar stack (DOC's own compose files, DOC's image, Calibre step stripped as upstream already did) | junit XML, logs |
| `frontend-rs` / `frontend-doc` (shards) | DOC's Playwright tests against each stack | reports |
| `ops` | M5 fault/soak/stats in containers | logs, csv |
| `table` | merge junit files into the per-file pass table | `pass-table.md` |

Cost control: validate locally first; batch fixes; record every run URL and what it proved in `data/doc-full-rs/checkpoint.md`.
The host never runs a container engine (a crash risk): every Docker step exists only in the workflow.

## 5. Local validation without a container engine (this host: 4 CPUs, 11 GB shared, `CARGO_BUILD_JOBS=2`)

hadolint and actionlint (static binaries), compose-spec JSON schema through `jsonschema`, nginx installed from the distribution package
extracted into `scratch/` (or built), Valkey from source, `mypy --strict`, the Rust tests, the Python service run as a plain process, the
frontend built with node, and DOC's e2e tests from DOC's own venv (Python 3.13.18) pointed at the plain-process stack by the P6 plugin.
Everything heavy lives in `scratch/` inside the worktree (not `/tmp`). One heavy job at a time; a status line goes to firstmate before any
command that needs more than 3 GB.

## 6. Parity work list (M3), known at the start (READ: P6 report, `tests/e2e`, `resource_lookup.py`)

1. Resources outside the Primary catalog: DOC resolves through `add_data_not_supplied_by_data_api` and the non-primary (`bg`) fetch plus
   repository-name rules (`update_repo_components`, `get_last_segment`); the P6 locator only used the Primary rows (7 failing tests).
2. Writer per-book repositories (`bys`, `bjz`; 5 failing tests): repository chosen per book, USFM file layout, `.txt` USFM.
3. TN chapter intro files absent (2 tests): tolerate and continue.
4. Status strings and result shapes the UI reads; the request fields the UI sends (`generate_epub`, `use_prince`, `show_rg_chapter_commentary`).
5. Every request shape in the e2e files: collected by running them against a recording target, then fixed in order of count.
6. HTML in DOC's own markup so `class="versemarker"` and friends parse (P6 already emits it; checked per test file).
7. docx through the seam (the docx e2e tests read the file with python-docx), PDF through Prince in CI.

## 7. Risks and what we do about them

| Risk | Mitigation |
|---|---|
| CI iterations are slow (image builds 10 to 20 minutes) | layer cache with a dependency layer for Rust and a separate font layer for Python; batch fixes; `workflow_dispatch` inputs to rerun a group |
| DOC's e2e tests need the real data API and real git host from the runner | accepted (DOC's own CI does the same); the pass table is relative to the FastAPI baseline run in the same job set, so flaky upstreams affect both |
| Parity is large (the P6 build passed 18 of 31 in one file; the tree has about 450 selected tests) | fix by failure class, not by test; keep a table of classes; stop adding shapes when the remaining failures are not request-shape problems |
| DOC's Dockerfile installs Calibre and the installer download fails sometimes | the baseline image build strips the Calibre step in CI (upstream already removed it); the strip is logged in the comparison |
| Prince cannot be tested on this host | CI only; WeasyPrint fallback covers the case it is missing |
| Memory on this host (11 GB shared) | one heavy job at a time, 2 jobs for cargo, frontend build measured, status line before > 3 GB |
| The unchanged frontend reads build-time env (`$env/static/public`) in places | read at M1; runtime vars set in compose; differences classified in M4 |
| Hand-written docx generator is not DOC's python-docx output | unchanged from P6 by decision; the captain's Word check is the acceptance |

## 8. Out of scope here

STET and Passages (tests excluded and listed in the report), randomized suites, the Sefer/kitchen fork question, merging or a pull request,
registry pushes, Kubernetes, opening anything in Word (the captain's check).

## 9. Deviations from this plan, and why (written as they happened)

1. **Which jobs run is chosen by markers in the commit message** (`[e2e]`, `[e2e-ui]`, `[ops]`, `[all]`), not by `workflow_dispatch` inputs and not by the `[ci: ...]` list of the first draft:
   `workflow_dispatch` only works for a workflow file on the default branch and I may touch no other branch. A run whose message contains `[e2e` is never cancelled by a later push.
   (The TypeScript build found the same limit; the job layout, the `ci/` scripts and the compose overrides of this build follow its, which was finished first.)
2. **No `volume-init` one-shot.** All application images run as uid 10001, so the first mount of a named volume cannot give it to the wrong owner; the gateway's and the test runner's
   mounts of the output volume are `nocopy: true` (the TypeScript build's run 5 found that a secondary read-only mount of the still-empty volume copies the image's ownership into it).
3. **The API role needs the PDF engine setting too** (`DOC_PDF_ENGINE=remote`, `DOC_PYWORKER_URL`): without it the API answers a typed 501 to every `generate_pdf` request. Found by the first local run of
   DOC's `test_api.py` (every PDF test failed with `KeyError: 'task_id'`); the compose file and `scripts/stack-p7.sh` now give the shared environment to both roles.
4. **The test runner is DOC's own API image built from DOC's own Dockerfile with only the Calibre step and the `mypy` lines removed** (`ci/make-baseline-dockerfile.sh`; DOC's Calibre installer failed in CI on 2026-11-24
   and upstream is removing ePub). The same image is DOC's FastAPI baseline.
5. **A request the catalog cannot resolve is no longer a permanent failure.** DOC drops it and builds the document (its own e2e test for `fa` expects a file); so does this build, and the findings say
   `resource_not_in_catalog`. Only "every resource that resolved is a USFM book its repository has no file for" is permanent (`resource_unavailable`). Two P6 tests that pinned the old behaviour were rewritten.
6. **`DOC_BOOK_NAME_RULE=written|doc`** (default `written`, the standing rule). DOC's own UI test for Amharic reads DOC's normalised book name (`2 ኛ ቆሮንቶስ`); with the rule on `doc` that test passes (proof job in CI).
7. **The worker id is unique per process start.** The container operations run found that a restarted container had the same pid, hence the same worker id, as its dead predecessor, and the reaper skipped the
   dead worker's job for good. Fixed (`w<pid>-<token>`) with a unit test; the operations run is repeated.
8. **Valkey image** `valkey/valkey:9.2.0-alpine` for the stack; the Rust job copies the `valkey-server` binary out of the Debian image `valkey/valkey:9.2.0` for the integration tests.

## 10. Status against the milestones (final)

| # | Milestone | Status and proof |
|---|---|---|
| M0 | plan, branch, workspace | done; this file written first, commit `32f98194` |
| M1 | container files, compose, gateway, remote Python seam | done; hadolint 2.15.1, actionlint, compose-spec schema, mypy `--strict`, `nginx -t` and the whole stack as plain processes (RAN on the authoring host) |
| M2 | first green CI: images build, stack healthy | done; run 2 https://github.com/linearcombination-ai/DOC/actions/runs/45740243168 (healthy in 26 s, smoke incl. Prince) |
| M3 | parity: DOC's `tests/e2e/doc` against DOC's FastAPI baseline | done; same run: **334 / 0 / 132 on both stacks, 0 tests differ** |
| M4 | the frontend end to end: DOC's Playwright tests | done; run 3: 17 / 1 / 10 against 18 / 0 / 10; the one difference is the book-name rule, proved by run 4 (18 / 0 / 10 with `DOC_BOOK_NAME_RULE=doc`) |
| M5 | operations in containers | done; run 5: every scenario passes (the first execution, run 3, found the worker-id bug); numbers in `COMPARISON.md` |
| M6 | report and handover | `../report.md` (data directory), README, RUNBOOK section 0, COMPARISON |

Risks that materialised: (a) the CI iterations were cheap (a full pass of DOC's API tests on both stacks takes 8 minutes), so the "images take 10 to 20 minutes" risk did not bite; (b) parity was large but fell to a small number of causes; (c) the container environment found a defect the host could not (section 9, item 7).
