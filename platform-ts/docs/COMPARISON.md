# COMPARISON: TypeScript build, P7 (the P6 headings, then three new sections)

Evidence tags: **RAN** (executed, output kept in a CI artifact, a test, or a log named here), **READ** (read in code or documents), **INFERRED** (reasoning only). DOC base commit `50de21cd3bab5c6f48961da64257118f6f0ead0e` (fork `linearcombination-ai/DOC`, branch `doc-dev.walink.org`); this build is branch `exp/platform-ts`. **P7 changes the evidence base**: everything about containers, DOC's own tests and per-container memory is new and was run in GitHub Actions (run URLs in "Docker and CI"); the sections inherited from P6 keep their P6 numbers (RAN on the shared 4-core, 11 GB host without docker, no container engine has ever run there) and say so, with a **P7** paragraph where the code behind them changed. Host numbers and CI numbers are never mixed in one table row.

## Run it

**P7: the container path (RAN in CI; the captain's own machine is the one thing not run here).**

```bash
cd platform-ts
docker compose up --build     # first build roughly 6 minutes here (cold cache, 4 vCPU runner), then open http://localhost:8001
```

In CI: images build in 4 to 175 s each (cache-dependent), the stack is healthy after 87 s of job time including image load, and the smoke test (health, lookups, docx, html, PDF with and without Prince, downloads, UI) passes in about 20 s. README has the shortest path; `docs/RUNBOOK.md` section 0 the container map. The paragraph and table below are the P6 plain-process path, still valid for development (`docs/LOCAL-STACK.md`).

Pinned tools (RAN, registries on 2026-11-25): Node 26.13.1 (nodejs.org), TypeScript 7.0.2, vitest 5.1.1, **bullmq 6.7.10**, **ioredis 6.0.0**, hono 4.13.21, zod 4.6.7, pino 10.4.0, **nodemailer 10.3.1** (2026-10-29, MIT-0), smtp-server 3.21.4 (tests only), Rust 1.100.0, wasm-bindgen CLI 0.2.130 (release binary), kitchen v0.1.24 `0090c0be` (read-only clone), Python 3.12.3 system / 3.13.18 DOC venv with WeasyPrint 68.1 and mypy 2.4.1, **Redis 8.12.0** and **Valkey 9.2.0** built from source (`make -j2 BUILD_TLS=yes MALLOC=libc`; Redis about 9 min with LTO under load, Valkey about 4 min; the Redis modules step was stopped on purpose; the version banners print DOC's git sha because the tarballs sit inside DOC's worktree).

| Step | Command | Time (RAN) |
|---|---|---|
| wasm, full + light + fault-injection modules | `bash scripts/build-wasm.sh` (`FAULT=1` for the third) | 2 min 26 s cold, 6 s and 4 s for the others; cargo peak RSS 636 MB |
| install | `npm ci` | about 11 s |
| local stack | `bash scripts/stack.sh start` | health answers within about 3 s |
| first response | `POST /documents_docx` then poll | cold (empty caches, local remotes) 786 to 855 ms; warm 97 to 255 ms (median 145 ms) |
| try-it path | README, exercised from a clean copy | see "Clean-copy rehearsal" below |

## Contract

* **All 11 endpoints of DOC's `doc_router` are served with the same paths** (RAN: contract tests; READ: `backend/doc/entrypoints/routes.py`): `/documents`, `/documents_docx`, `/task_status/{id}`, `/language_codes_and_names`, `/shared_book_codes/..`, `/resource_types/..`, `/book_codes_for_lang/..`, `/book_codes_for_lang_from_usfm_only/..`, `/chapters_in_books`, `/health/status`. Added: `/findings/documents/{key}` and the `x-doc-findings` header, `/metrics`, `?strict=1` on health, `?findings=1` on lookups. **Not served: STET and Passages routes** (`stet_router`, `passages_router`; out of this slice).
* Status strings are DOC's own, matched literally by `TaskStatus.svelte`; two more DOC strings exist for the other task and are now used: `Converting to PDF`, `Sending email` (RAN: golden test). The cache key is DOC's (P5: same string as DOC's FastAPI for the same request; goldens `en_ulb_mat_70c7420bb621`).
* Typed errors: 422 in DOC's legacy body; 503 `{kind, transient:true}` with `Retry-After` when the catalog is down and nothing is cached; 503 `queue_unavailable`; 503 `disk_space_low`; 413 `request_too_large`; 501 `not_implemented` (PDF off). All the typed bodies also carry `detail` so the frontend's `data.detail` shows text (found by the frontend check).
* One deliberate change: de-duplication id is `kind:key:pdf-flag:recipient`, not the key alone. With the key alone, two people asking for the same document by email shared one job and one of them got nothing (found while writing the email tests; Celery has no de-duplication, so DOC does not have this).
* **Frontend types (G, RAN):** `generated/api.d.ts` regenerated from the contract (openapi-typescript 7.13.0); `test/frontend-types.check.ts` restates the call sites of `books`, `resource_types`, `transfer`, `settings/GenerateDocument.svelte` (poll and generateDocument) with the generated types and `tsc --noEmit` passes. Three **mismatches**, each pinned by `@ts-expect-error` so a change makes `tsc` fail: (1) FAILURE's `result` is an object, a typed client treating `result` as a string would be wrong (the UI never reads it); (2) the 422 body has no `detail`, so the UI shows no error text on a rejected request (same as DOC today); (3) the UI's `.epub` download link points at a file that is never produced (ePub removed). The frontend source is unchanged.
* **DOC's own API e2e tests pointed here (F, RAN, real network):** `tests/e2e/doc/test_api.py`, 31 tests run, 11 skipped by their markers. The tests use FastAPI's in-process `TestClient`; `tools/e2e/doc_e2e_shim.py` (a pytest plugin, mypy strict) swaps it for an httpx client on a URL. Same tests, same host, same network, DOC's FastAPI + Celery without docker against this build:

| | DOC FastAPI | this build |
|---|---|---|
| passed / failed (skipped 11) | 6 / 25 | 4 / 27 |
| both pass (4) | docx en ulb+tn col, docx en bc col, docx or/en TN cases | same |
| DOC passes, this build fails (2) | `test_fa_ulb_tn_lbo_1c_chapter`, `test_en_ulb_eph_bys_reg_eph_blo_1c_chapter_docx` | see below |
| both fail (25) | 17 to 21 `No verses found in HTML` and 8 `celery FAILURE` (no `prince` binary on this host; DOC's USFM sidecar also refused connections during parts of the run) | see below |

Classification of every difference (READ the result files `results/e2e-*.txt`): (a) **HTML structure**: 21 of this build's failures are the check `class="versemarker"` in `<key>.html`; this build's HTML uses the P3b "proposed look" semantic classes, not DOC's markup. A real contract difference: the tests assert on HTML that the captain's look decision has replaced. (b) **`reg` repositories laid out per chapter** (`Bayan/bys_eph_text_reg`: `01/01.usfm`...): the shell's USFM finder reads whole-book files only (`resource_unavailable`, permanent): not implemented, the cause of `test_en_ulb_eph_bys...docx`. (c) **Prince absent**: 8 DOC failures; this build would use WeasyPrint. (d) The first run of this build died of its own memory ceiling on whole-book PDFs (`docx_worker_memory`, 16 attempts, WeasyPrint reached 360 MB on a 1.6 MB html): fixed by a per-format ceiling (`PDF_MEMORY_CEILING_MB` 2500, docx 700) before the second run, which is the one tabled. (e) DOC's side is impaired by this host (sidecar), so its 6/31 is not DOC's real pass rate; I did not get a clean DOC baseline. The honest summary: **this build passes the docx path and not the HTML-markup or per-chapter-repository assertions**.

**P7 changes (RAN: DOC's own tests pass on them, see "DOC e2e pass table").**

* **HTML and PDF are in DOC's own markup and CSS by default** (`OUTPUT_LOOK=doc`): DOC's header templates (`header_enclosing.html`, the compact variant for `1c_c` and `2c_sl_sr_c`) rendered by jinja2 in the Python seam exactly as DOC renders them, `span.verse` holding `sup.versemarker`, `<hr/>` where a part asks for a separator, DOC's chapter and book headings (`h2.book-name`, `h1.book-name`). The captain's proposed look stays available (`OUTPUT_LOOK=proposed`). The **docx keeps the proposed look's Word styles in both settings** (the template-free generator is built for it): so a docx and a PDF of the same request now differ in look. Word was not opened: that is the captain's check.
* **Title page strings are DOC's**: `get_languages_title_page_strings` is ported (first two languages, "Old and New Testaments" / "Old Testament" / "New Testament" naming, canonical book order), with DOC's localized book names: the USFM's own markers, else the repository's manifest (yaml or json), else `front/title.txt` of a repository whose name has more than two parts, else the English name; for a Writer repository the stitched USFM is read first. DOC's Playwright tests assert these strings.
* **DOC's USFM repair rules** (`usfm_error_detection_and_fixes.py`, 13 regular-expression rules applied chapter by chapter, for the 98 languages with a known defective resource, `CHECK_USFM` and `CHECK_ALL_BOOKS_FOR_LANGUAGE` as in DOC's `.env`) are ported into the core with `fancy-regex` (look-behind). A document built from repaired text carries a `usfm_repaired` finding.
* **A request ends with a document.** Nothing resolves in the catalog (a language it does not have): an empty document plus `ResourceNotInCatalog` findings (DOC does the same; its test asks for exactly that). A book absent from its repository: an empty book plus `UsfmNotFound` (DOC's behaviour); if **every** requested book is absent, a permanent `resource_unavailable` (nothing to build). On the last attempt a repository that cannot be fetched is left out with a `RepoNotFetched` finding instead of failing the request; if none could be fetched it stays the transient error.
* **PDF**: `PDF_ENGINE=auto` (default) uses Prince when the request sets `use_prince` and the binary exists, else WeasyPrint with a `PdfEngineFallback` finding; `prince`, `weasyprint`, `off` (typed 501) as before.
* The 11 endpoints, status strings, result shapes and the cache key are unchanged from P6 (the frontend types check still passes, RAN in the unit job).

## Core boundary

* Same as P5: three pure ops, JSON in and out (`job_key`, `job_parse`, `job_assemble`), no I/O, no clock. P6 changes: `key_json` moved into `doc-request`; `doc-wasm` has a `jobs` feature so the **API loads a light module (`pkg-api` 782 KB, request keys and book lists) and the worker the full one (`pkg` 2,972 KB)**; a test-only `fault-injection` feature adds `debug_spin`. Rust edited: 3 files (RAN: `git diff`).
* **API memory effect (RAN, `results/api-memory.txt`, 3 runs each, host load 4.5):** light 139 MB at start vs full 144 MB, after normalising a 21,000-row catalog 292 vs 289 MB, after 300 mixed requests 308 vs 310 MB. **The light module saves about 5 MB; the catalog and Node dominate.** Reported as found: "keep the API light" is satisfied by not loading the interleaver, not by a memory win.
* Catalog normalisation runs on the module's thread in both processes (A4, RAN): worst event-loop gap while normalising 21,000 rows **363 ms inline vs 46 ms on the thread** (the 46 is the JSON in and out on the main thread).

**P7 changes (RAN: `cargo test` workspace green, wasm built in CI).** Rust edited since P6 (RAN: `git diff d87ba32f..HEAD -- core/crates`, 12 files, +955 lines): `doc-books` (`resource_lookup_dto` over the Primary catalog plus DOC's own six entries; the book-name rules in `bookname.rs`), `doc-canon` (DOC's chapter markup, `usfm_fixes.rs` 284 lines and its generated table of 208 defective (language, type, book) triples), `doc-job` (title strings, Writer stitching, names, repairs, `doc-job` now depends on `doc-books`). One new external dependency: `fancy-regex` 0.16 (pure Rust, look-behind), which adds about 170 KB to the full wasm module: **3.2 MB full, 0.8 MB light** (was 3.0 / 0.8). Pure ops, JSON in and out, no I/O and no clock, as before; the shell lists and reads the files the core asks for (repository listings and the manifest and title texts for the name rules).

## Job runner

BullMQ 6.7.10 on Redis 8.12.0 or Valkey 9.2.0; pipeline function unchanged in shape (`runDocumentJob(deps, data, ctx)`), 1,215 lines of job-runner TypeScript. P6 adds: per-repository **cross-process single flight** (Redis `SET NX PX` lease, renewal every lease/3, fencing number from `INCR`, publish refused when a newer generation exists; falls back to cloning when Redis is unavailable). RAN: two worker processes, empty cache, slow clone: **2 clones for 2 repositories with the lock, 4 without** (`test/repolock.test.ts`); holder SIGKILLed mid-clone: lease expires, the other worker finishes the job; fencing test: a holder that lost its lease is fenced out and adopts the newer copy. Residual window (READ, in `repolock.ts`): check and rename are two steps; both results are complete clones.

**P7 changes.** The locate stage uses the core's `resource_lookup_dto` (DOC's resolver, including the augmented entries); the provision stage degrades on the last attempt as described under "Contract"; the parse stage passes DOC's title inputs and the name-rule inputs; the assemble stage picks the markup (`as_built` for HTML and PDF, the proposed look for docx). The queue, locks, stalled-job recovery and retry policy are P6's and ran unchanged in containers (see "Fault tolerance").

## Docx seam

Two modes behind `DocxRunner`, switch `DOCX_MODE`: `process` (one Python process per job) and `persistent` (pool of `docx_server.py`, JSON lines on stdin/stdout, default). Supervision (RAN, `test/docx.pool.test.ts`, 11 tests): same pid across jobs; crash mid-job fails that job transient and the process is replaced; deadline kills the process group and replaces it; abort kills like a deadline; memory ceiling kills mid-job (`docx_worker_memory`, transient); recycle after N jobs; idle crash refilled; N jobs on M workers queue (never more than M processes); `close()` leaves none. Formats: docx, html, pdf (WeasyPrint through one function `pdf_bytes`, the Prince swap point).

**Cost per job, measured through the real pipeline, 30 jobs each, one at a time, different documents (RAN, `results/docx-bench.txt`, host load 4.5 so absolute numbers are pessimistic):**

| mode | end to end median (p95) | docx stage median (p95) | Python generation median |
|---|---|---|---|
| process | 365 ms (921) | 306 ms (848) | 87 ms |
| persistent | **110 ms (537)** | **67 ms (478)** | 64 ms |

The persistent pool removes about 240 ms of interpreter start and imports per job. Soak memory of its processes is below.

**P7 changes.** The seam builds HTML with DOC's templates (jinja2 3.1.6, `python/templates/html/`, copied from `backend/templates/html/`), and the PDF engine is now a choice: `pdf_bytes(document, job)` runs Prince (`prince <html> -o <pdf>` on a temporary file, its own time limit, the process group killed by the supervisor on a deadline like the Python process) or WeasyPrint, and reports which one ran and why (`engine`, `note` in the result, a `PdfEngineFallback` finding in the document's findings). Prince ran in CI (smoke test, `use_prince: true`, no fallback finding: 69 KB PDF). The Python seam is 408 lines + the generator 769; mypy `--strict` and the no-`typing.Any` check pass in the static job.

## Fault tolerance

**P6 evidence, carried over** (the P7 container drills are the next section after this table's notes). All RAN on real processes (Redis or Valkey, git, Python, SMTP server), no fakes; final result per server in P6's `results/suite-redis8.txt` and `results/suite-valkey9.txt` (kept in `data/doc-p6-ts/prototype/results/`, not copied into P7; the same suite runs against Valkey 9.2.0 in the CI unit job).

| Fault | Test | Result |
|---|---|---|
| catalog down longer than client retries | `job.fault` | job retries, document produced |
| permanent error (every requested book missing from its repository; **P7**: one missing book among others is an empty book and a finding, as in DOC) | `job.fault` | 1 attempt, `{code, retryable:false}` |
| invalid request | `job.fault`, `config` | 422 legacy body, never queued |
| hung docx worker (ignores SIGTERM) | `job.fault`, `docx.pool` | killed at deadline, pid gone, job retried or failed typed |
| hung git clone | `job.fault`, `acquirer.fault` | killed at deadline, group gone |
| hung core call (JS spin) | `core.cancel` | thread terminated, attempt 2 succeeds, event loop gap 32 ms |
| hung core call (loop inside wasm) | `core.cancel` (fault-injection build) | V8 `terminate()` interrupts it; 2 terminations over 2 attempts, gap 14 ms |
| worker SIGKILL mid-job | `job.process` | stalled job recovered once, 1.8 s after kill; orphaned Python gone in 88 ms |
| worker SIGTERM mid-job | `job.process` | exit 0 after finishing the job (2.7 s) |
| poison job kills every worker | `job.process` | failed `job_stalled`, not looped |
| Redis stop and restart / SIGKILL with AOF | `job.process` | job completes once, valid output |
| lock holder SIGKILLed mid-clone | `repolock` | lease expires, other worker takes over |
| lock lease lost (fencing) | `repolock` | stale holder fenced out |
| Redis unreachable for the clone lock | `repolock` | clones without it |
| persistent Python: crash, hang, memory ceiling, idle death | `docx.pool` | typed transient, replaced, next job works |
| SMTP refuses recipient / drops after DATA / process died mid-send / 20 concurrent | `email` | one message at most; findings; at-most-once |
| retry after email sent; redelivery of a sent job | `email` | no second email |
| disk below floor | `config`, `obs` | job waits (retries); API refuses new jobs at half the floor (503, Retry-After) |
| Redis down: metrics, health | `obs` | endpoints answer (`doc_redis_up 0`, degraded) |
| bad configuration, bad preflight | `config` | exit 78 with every problem listed |
| 24 concurrent jobs | `tools/burst-n.ts` | 0 failures, wall 1.1 s (first run 2.2 s) |
| 31-minute soak, 6,677 requests | `tools/soak.ts` | 0 failures |

**Valkey (B, RAN):** Valkey 9.2.0 built from source and the **whole suite run against it** with no code change: see the counts in "Suite results". Licences read from the source trees: Redis 8.12.0 `LICENSE.txt`: RSALv2 / SSPLv1 / AGPLv3 at your choice (7.2 and older were BSD); Valkey 9.2.0 `COPYING`: BSD 3-Clause. BullMQ does not name Valkey; it worked. Persistence settings documented in RUNBOOK section 3.

### Suite results

Final runs (`bash scripts/run-suite.sh <server> <label>`, 15 files, 155 tests, files one at a time, host load about 2):

| server | result | duration | file |
|---|---|---|---|
| Redis 8.12.0 (built from source) | **155 passed** | 176 s | `results/suite-redis8.txt` |
| Valkey 9.2.0 (built from source) | **155 passed** | 169 s | `results/suite-valkey9.txt` |

History, stated plainly: earlier runs during development saw flakes under host load (one run of each server had 1 to 2 failures that passed in isolation: a fixed 600 ms sleep waiting for a TLS Redis to start, a timing test of the output cache window, a per-request deadline test of the P4 acquirer). The TLS wait now polls the port; the other two are timing tests and passed in the final runs of both servers. A Redis `SIGKILL` + AOF replay test and the Redis stop/restart test run on both servers.

**P7: the same faults in containers (RAN, CI job `ops`, run 9: https://github.com/linearcombination-ai/DOC/actions/runs/45732097074; `ci/ops.sh`, `ci/loadgen.py`; real network, the compose stack with its limits).**

| Scenario | What was done | Result |
|---|---|---|
| limits applied | `docker inspect` of every service | memory limits valkey 256 MB, api 768 MB, worker 3 GB, gateway 128 MB, frontend 512 MB; CPUs 1 / 1 / 2 / 0.5 / 1; restart `unless-stopped`; stop timeouts api and valkey 20 s, worker 135 s |
| SIGTERM the worker mid-job | `docker compose stop -t 130 worker` while a job was in "Locating assets" | log "shutting down: no new jobs, draining the running ones", then "drained; bye"; **exit 0 after 11 s**; the job finished (SUCCESS) |
| `docker kill -s KILL` the worker mid-job | same job type | the engine does **not** restart a killed container (`docker kill` is a manual stop): the operator starts it; the job was recovered ("job stalled: its worker stopped renewing the lock") and the document arrived; 51 s end to end |
| crash of the worker process inside its container mid-job (what an out-of-memory kill or a segfault looks like) | `kill -9` of the node process from inside | **`unless-stopped` restarted the container (restart count 0 to 1), healthy again after 7 s**; the job was recovered as stalled after the lock expired and the document arrived; 48 s end to end |
| Valkey restart | `docker compose restart valkey` with one finished job and one in flight | AOF replay: the old record still answers, the in-flight job finished, a new job works; 8 s |
| API crash, then SIGTERM | `kill -9` inside, then `docker compose stop api` | restarted by the policy, healthy after 6 s; a new job works; SIGTERM exit 0 in under 1 s |
| 24 different documents at once | `ci/loadgen.py burst` | **0 failed, 2.8 s wall**, p50 1.5 s, p95 2.0 s, max 2.5 s |
| rollback to another image tag | `IMAGE_TAG=rollback docker compose up -d --no-build api worker`, then back | same volumes and queue: the earlier document still served (HTTP 200), a new document SUCCESS, files 60 to 62 |
| 10-minute soak | 1 request/s, three in four repeats (cache hits), one in four new documents, docx, html and PDF | **600 requests, 0 failed**, p50 0.5 s, p95 1.5 s, max 3.0 s, **0 container restarts**; memory flat (see "Memory ...") |

The first two ops runs (7 and 8) had bugs in my scripts: run 7 went green with every scenario broken (a pipe hid the exit code; fixed with `shell: bash`, which sets `pipefail`) and run 8 failed one check because I had expected `docker kill` to be restarted (it is not; the scenario now crashes the process inside the container, and keeps the `docker kill` case as a recovery test). Both are in the log above because the numbers of run 9 depend on them. The P6 fault suite (below) was run again as the unit job on every CI run, against Valkey 9.2.0 from the official image (RAN; run 7: 190 s); the TLS test needs a TLS-built server and passes in CI, not on the authoring host.

## Observability

RAN (`test/obs.test.ts`, 11 tests). One structured line per stage (`stage finished`: stage, ms, ok) and per attempt (`attempt started`, `attempt failed ...retry|giving up`) with `reqId`; the request id travels HTTP (`x-request-id`, echoed) to the worker to the Python process (`python: render start|done`, field `req_id`) and into email headers; checked in both docx modes. `/metrics`: Prometheus text, fleet-wide through Redis counters (queue depth, oldest waiting, stage histograms, attempts, failures by code, stalled recoveries, clone counts, cache hits, catalog refreshes and failures, worker liveness, docx pool, email); each sample line validated against the text-format grammar. `/health/status`: Redis (version, latency, AOF, eviction policy), queue lag, workers, docx pool, last successful catalog fetch, disk, and `problems`. RUNBOOK covers symptoms, log dictionary, drain, replay (`tools/queue-admin.ts`), Redis loss, metric alarms.

## Performance

Same client, same local data, same host, DOC's FastAPI + Celery (concurrency 2) without docker vs this build (RAN, `results/bench-*.jsonl`, `bench-p6.sh`; load 1.9 to 2.6):

| scenario | DOC | this build |
|---|---|---|
| cold, local remotes (3 runs) | 1,640 / 1,869 / 2,302 ms | 786 / 805 / 855 ms |
| warm (document removed, repos cached), median of 10 | 1,122 ms | **145 ms** (97 to 255) |
| cache hit, median of 10 | 52 ms | 50 ms |
| 6 different documents at once, wall, 5 runs | 2.85 to 3.37 s (median 2.88) | 0.38 to 0.44 s (median 0.40) |
| 24 different documents at once, 3 runs | not measured | 2.15 / 1.10 / 1.09 s, 0 failures |
| idle memory of the stack | uvicorn 82 + Celery 260 + sidecar 52 to 83 + Redis 12 MB (about 410 to 440) | API 143 + worker 212 (with 2 warm Python processes) + Redis 13 MB (about 368) |
| peak during the warm/hit runs | Celery tree 360 MB | worker tree 267 MB, API 154 MB |
| **real network, cold, en ulb+tn Jud, real catalog and content host** | 14.1 s | 15.9 s |
| real network, warm (3 runs) | 0.91 to 0.99 s | 0.12 to 0.18 s |

The warm and burst gains come from the docx generator and the persistent Python pool, not from the queue (P5 finding, confirmed: per-job mode costs 365 ms vs 110 ms). The real-network cold time is dominated by the clone (DOC 14.1 s, this 15.9 s); this build was not faster there.

**Soak (F, RAN, `results/soak/`)**: 31 minutes, ~3 requests/s, API + 2 worker processes with persistent pools (2 Python each) + Redis; mix 34% docx, 15% html, 4% PDF (WeasyPrint), 25% duplicates (three identical submissions each), 8% lookups, 4% 422s, 4% unknown ids, 6% metrics and health scrapes; output TTL 1 minute so documents rebuild. **6,677 requests, 0 failures, 287 rejected as designed, queue empty and health ok at the end, no Python process and no scratch directory left after SIGTERM, 33 file descriptors at start and end.** Latency (ms, p50/p95): docx 161/195, html 161/202, pdf 787/1,377, duplicates 17/176. Memory (slope after a 5-minute warm-up): API 203 to 157 MB (−0.27 MB/min), worker 1 172 to 178 (+0.27), worker 2 169 to 175 (+0.25), Python children flat (180.0 vs 179.8 MB and 132.8 vs 132.4 MB; the 1.7 MB/min slope the fit gives for one is a transient in the early samples, first-to-last difference is 0.3 MB), **no leak in the processes**; **Redis grew 15 to 22 MB (+0.25 MB/min)**: completed job records kept 24 h (`KEEP_JOBS_SECONDS`), expected and bounded by the retention, named here as the one growing thing. Host load rose to 4.4 at times (the other build's worker).

**P7 (RAN, CI; different machines from the P6 host, so not comparable with the table above).** The same DOC tests on both stacks in one run (pytest wall seconds per group of the three, `pytest_seconds` in the artifacts): TypeScript stack **141 / 169 / 191**, DOC's FastAPI stack **181 / 204 / 202**; Playwright per shard of four: TypeScript **58 / 31 / 51 / 19** s, DOC's stack **119 / 75 / 112 / 53** s (run 7). Latency through the containers and the gateway from the 10-minute soak: p50 0.5 s, p95 1.5 s, max 3.0 s for a mix of one new document in four; the burst of 24: p50 1.5 s, 2.8 s wall. DOC's cold real-network time and this build's are both clone-bound (P6: 14.1 s vs 15.9 s); that did not change. No container-level DOC soak was run (not measured).

## Size

`bash tools/loc.sh` (RAN, lines with comments and blanks): Rust core copied from P3b/P4 7,676 (+360 generated tables), Rust entry points 266; TypeScript: contract 238, app/mains/config 683, core wrapper + catalog + acquirer + lookup + util 1,177, job runner 1,215, docx seam 637, core thread 122, clone lock 108, email 160, observability 276; Python seam 323 + generator 769 (stdlib only); tests 3,210 (141 cases in 15 files); tools and scripts 1,197; generated 1,657 + 928. Dependencies added in P6: `nodemailer` (runtime; maintained by the Nodemailer project, 15+ years, MIT-0, v10.3.1) and `smtp-server`, `@types/*` (tests); production tree otherwise as P5 (BullMQ 6.7.10, ioredis 6.0.0, Hono 4.13.21, zod 4.6.7, pino 10.4.0). Rust: 85 crates in the wasm build as P5. `any`: none in TypeScript (grep; the casts `as never` for nodemailer's logger option and `as object` for ioredis' constructor overload are explained in comments); `typing.Any`: none in Python; **mypy 2.4.1 `--strict` clean over 10 Python files** (`scripts/mypy.sh`).

**P7 (RAN: `bash tools/loc.sh`, lines with comments and blanks).** Rust core copied from P3b/P4 8,431 (+360 generated tables); Rust entry points 454; TypeScript: contract 238, app/mains/config 690, core wrapper + catalog + acquirer + lookup + util 1,208, job runner 1,300, docx seam 647, core thread 122, clone lock 108, email 160, observability 276; Python seam 408 + generator 769 (stdlib only); tests 3,249 TS lines (142 cases in 15 files) + 16 Rust tests + the new core tests (title strings, repairs); tools and scripts 1,243; container and CI files: 3 Dockerfiles (shell, worker, gateway; DOC's own frontend and test Dockerfiles are used unchanged) and 4 compose files (the system, the e2e override, two baseline overrides), 10 CI helper scripts and compose overrides in `ci/` (735 lines), one workflow of 732 lines. The change since the P6 base commit (`d87ba32f`): 57 files, +4,238 lines in `platform-ts/` outside the lock file (RAN: `git diff --stat d87ba32f..HEAD`, as of commit be4ba9b7).

## Build and test cycle times

wasm 2 min 26 s cold (3 modules: +6 s, +4 s), `tsc --noEmit` about 6 s, full suite about 3 to 4 minutes (155 tests, files one at a time), the Redis build about 9 min and Valkey about 4 min (once), `npm ci` 11 s. No build step for TypeScript (Node strips types).

**P7 (RAN, CI).** Whole workflow run 7: **7 min 23 s** with everything on (see "Docker and CI"). Unit job (wasm build cached, `npm ci`, the whole vitest suite with a Valkey 9.2 binary from the official image) 190 s; static checks 38 s; smoke 87 s; image builds 4 s to 175 s. The local loop on the authoring host (cargo test, wasm build, `tsc`, DOC's e2e files against plain processes) is minutes, not hours, which is what made 9 CI runs enough.

## Weaknesses

1. **Parity rests on ported rules.** DOC's USFM repair rules are Python `re` patterns; the port uses `fancy-regex` and was checked on the 208 repair tests and one hand comparison of the repaired chapter text against Python's (identical), not on every language and every book: differences in Unicode digit handling, in the backtracking limits, or in a pattern built from user text (`fix_missing_space_after_number` builds a regular expression from the matched text without escaping it, as DOC does) are INFERRED to exist at the edges.
2. **Writer repositories are stitched by kitchen's code, not by DOC's `combine_usfm_files`.** Same result on the tests; the one difference found (kitchen writes a `\cl` label that DOC's concatenation does not, which changed what the repair rules decided) shows the two paths are not identical. The core now removes chapter labels before repairing, as DOC does.
3. **docx and PDF no longer look alike by design**: HTML and PDF are DOC's markup and CSS; the docx keeps the proposed look's Word styles. The e2e docx tests are text-based and pass. Which look the docx should have is the captain's call after opening it in Word.
4. **Two runtimes in the worker image** (955 MB: node, Python, Prince, fonts, WeasyPrint) and a node 26 requirement; the api image is 392 MB. DOC's single image is 1,574 MB.
5. **Valkey (or Redis) is a hard dependency** (the queue and the status store). Valkey 9 is BSD-3 and the whole suite and all of CI run on it; Redis 8's licence (RSALv2 / SSPLv1 / AGPLv3) is the operator's choice, and the Redis 8 suite run is P6's (host), not re-run in containers.
6. **Prince runs unlicensed** (non-commercial mode, notice on page one), exactly like DOC's image. A production deployment that wants no notice needs a licence: not decided here.
7. **`docker kill` is not self-healing.** The restart policy restarts crashes and out-of-memory kills (RAN) but not a container an operator killed or stopped by hand; documented in the RUNBOOK.
8. **The tests depend on the live catalog and content hosts** (as DOC's own CI does): an outage fails both stacks in the same run. I did not retry failed groups, and none failed in the green runs.
9. **The e2e totals include 132 skipped test cases that are DOC's own skips**; "334 passed" is the real count of tests that ran.
10. At-least-once with an at-most-once email (P6), metrics are Redis-backed and reset if Redis loses data (P6), the clone lock needs a shared assets volume (P6; compose provides it).

## What was not done

* **STET and Passages** (routes, `tests/e2e/stet`, `tests/e2e/passages`, the four Playwright files): out of this phase by the brief; listed as excluded above.
* **Word was not opened on any output**, and the OOXML validator was not re-run on the P7 docx (the generator is P5's); the captain's visual checks and acceptance are the open items.
* **`docker compose up` on the captain's own machine** (including arm64: the Dockerfiles have an architecture switch for wasm-bindgen and Prince, built only for x86_64 in CI): not run, INFERRED.
* **DOC's `tests/unit`, the randomized, `english_randomized` and `all` e2e tests** (DOC's CI excludes the last three by its marker expression), and DOC's `make` targets.
* **Memory limits being hit** (an OOM kill of the worker), CPU throttling, a soak longer than 10 minutes in containers (P6 ran 31 minutes on the host), faults injected during the soak, more than 24 documents at once, a multi-host or multi-worker container test (the clone lock was tested with two worker processes in P6), image vulnerability scanning, a registry push (forbidden by the brief), Kubernetes.
* **The optional Effect comparison**, and the Rust shell build (the other build, paused by the captain; nothing of it was read this time beyond its checkpoint, which had no findings to take).
* A clean-room `docker compose up` rehearsal from a fresh clone on a second machine: the CI runs start from a clean checkout on a fresh runner, which is the same thing for the compose path but not for the README text.

## Docker and CI

Everything below ran in GitHub Actions on the fork `linearcombination-ai/DOC`, workflow `platform-ts` (`.github/workflows/platform-ts.yml`, branch `exp/platform-ts`); **no container engine was ever run on the authoring host** (RAN). Images are built on the runner and never pushed; there are no secrets. DOC's own `docker-publish.yml` is switched to manual runs on this branch (it pushes to a registry and is red by design on the fork).

**Runs that mattered** (each URL is a run of this workflow; "what it proved" is what the run showed, red runs included):

| # | Run | Commit | Result | What it proved |
|---|---|---|---|---|
| 1 | https://github.com/linearcombination-ai/DOC/actions/runs/45679607716 | 9df12a94 | red | all four images build; unit job (Valkey 9.2) green; static red (mypy needed `fastapi`); stack up red: api crash-looped on `PDF_ENGINE=auto` (config enum lacked it), worker on a missing `libatomic.so.1` |
| 2 | https://github.com/linearcombination-ai/DOC/actions/runs/45721845728 | e23d3745 | red | static and unit green; stack up red: `EACCES` on `/data/*` (named volumes not writable by the non-root users) |
| 3 | https://github.com/linearcombination-ai/DOC/actions/runs/45722922937 | bb8e81bc | **green (M2)** | stack healthy through the gateway, smoke: health, lookups, docx, html, PDF, downloads on :8089, UI on :8001 |
| 4 | https://github.com/linearcombination-ai/DOC/actions/runs/45723732331 | 1170e933 | red | first e2e run: DOC baseline image and compose stack build and start; pytest refused DOC's `--failed-first` because my override passed `-p no:cacheprovider` |
| 5 | https://github.com/linearcombination-ai/DOC/actions/runs/45724725370 | e22b9e5b | cancelled by me | **DOC's FastAPI baseline: 334 passed, 0 failed, 132 skipped**; TS stack: every job failed `EACCES` at placement (a read-only secondary mount of the empty output volume copied the DOC image's ownership into it) |
| 6 | https://github.com/linearcombination-ai/DOC/actions/runs/45726295761 | 24d04679 | red (as expected) | first full pass tables: API 328 passed / 6 failed / 132 skipped (6 = DOC's USFM repair rules not ported), Playwright 16 / 2 failed / 10 (2 = title strings) |
| 7 | https://github.com/linearcombination-ai/DOC/actions/runs/45729013524 | 77d125e2 | **green (M3, M4)** | **both stacks identical: API 334 passed / 0 failed / 132 skipped, Playwright 18 / 0 / 10** |
| 8 | https://github.com/linearcombination-ai/DOC/actions/runs/45729996638 | ffbb9b02 | red | first working ops run: all scenarios pass except my own wrong expectation (`docker kill` is a manual stop that the restart policy does not undo); also showed the first two ops scripts had bugs (run 7: the job went green with every scenario broken, because a pipe hid the exit code) |
| 9 | https://github.com/linearcombination-ai/DOC/actions/runs/45732097074 | be4ba9b7 | **green (M5)** | every ops scenario passes (see "Fault tolerance"): drain, crash and restart policy, Valkey restart, burst, rollback, 10-minute soak; versions of every tool inside the containers |

**CI wall time.** Run 7 (everything: static, unit, 4 + 3 image builds, smoke, API e2e on both stacks in 3 groups each, Playwright on both stacks in 4 shards each, ops build only): **7 min 23 s** from push to the last job (RAN; created 00:28:45Z, finished 00:36:08Z). Per job in run 7 (seconds, whole job including setup and image load): static 38, unit (Valkey, whole vitest suite) 190, smoke 87, API e2e TS groups 288 / 255 / 277 and DOC groups 274 / 304 / 308, Playwright TS shards 195 / 200 / 186 / 202 and DOC shards 278 / 234 / 234 / 176.

**Image sizes** (`docker image ls`, RAN; uncompressed):

| Image | This build | DOC's stack |
|---|---|---|
| api (HTTP shell, light wasm) | **392 MB** (node 26 slim, git, no Python) | DOC's single image **1,574 MB** serves both its API and its Celery worker |
| worker (job runner + Python seam + Prince + fonts) | **955 MB** | (same image) |
| gateway (nginx) | 48 MB | fileserver 142 MB (nginx) |
| USFM parser sidecar | none (the core parses USFM) | 294 MB (.NET) |
| frontend (DOC's own, unchanged) | 968 MB | 968 MB |
| queue | `valkey/valkey:9` (not measured here) | `redis:latest` + Flower (not measured here) |
| total of the above | **2,363 MB** | **2,978 MB** |

**Build times** (seconds; `docker/build-push-action` with the GitHub Actions layer cache, RAN): cold cache (run 1, run 4) api 102, worker 175 (Rust to wasm, node modules, Prince, fonts), gateway 8, frontend 119; DOC baseline api 140 (without its Calibre step), usfmparser 59, fileserver 5. Warm cache with a source change (runs 3, 7): api 8 to 67, worker 64 to 72, gateway 4 to 9, frontend 6 to 12; DOC api 52, usfmparser 5, fileserver 6. The captain's first `docker compose up --build` has no cache: expect roughly the cold numbers added up (about 6 minutes of building here, on a 4 vCPU runner), INFERRED.

**Prince and its licence (READ + RAN).** Prince 16.1-1 is installed the way DOC's own Dockerfile installs it: the `.deb` from princexml.com, no licence file. Without a licence Prince works in its non-commercial mode and puts a notice on the first page; DOC runs the same way today. The build prints a warning and continues with WeasyPrint when no Prince package exists for the CPU architecture. The seam asks for Prince when the request says `use_prince` (or `PDF_ENGINE=prince` for everything) and returns a `PdfEngineFallback` finding if it could not use it. RAN: the smoke test makes a PDF with `use_prince: true` and fails when that finding exists (runs 6 and 7: passed, 69 KB). Whether to buy a Prince licence is a commercial decision this work does not take.

**Tool versions** (RAN, from the runs): Node 26.13.1, Rust 1.100.0, wasm-bindgen 0.2.130, Python 3.13.18 in CI (3.13 in the images), WeasyPrint 68.1, jinja2 3.1.6, fancy-regex 0.16, Valkey 9.2.0 (`valkey/valkey:9`), Prince 16.1-1, hadolint 2.15.1, actionlint 1.7.12, mypy 2.4.1, Playwright image v1.41.1 (DOC's `frontend/testsDockerfile`), compose schema from the compose-spec repository. Docker engine 28.0.4 (server and client) and Docker Compose v2.38.2 on the runner (`ubuntu-24.04`); inside the containers: node v26.13.1, Python 3.13.18, Prince 16.1, WeasyPrint 68.1, jinja2 3.1.6, Valkey 9.2.0 (`versions.txt` in the `smoke-results` artifact of run 9).

## DOC e2e pass table

DOC commit `50de21cd` (upstream base of the fork's `doc-dev.walink.org`); the tests are DOC's own, unchanged, run by DOC's own test-runner image (DOC's Dockerfile minus its Calibre step and its `mypy` lines: `ci/make-baseline-dockerfile.sh`). Same marker expression as DOC's CI (`not all and not randomized and not english_randomized`), same `pytest-xdist` and `pytest-split` flags (3 groups), same real network for both stacks (catalog and content hosts), same run. The pytest plugin `tools/e2e/doc_e2e_shim.py` points DOC's `TestClient` at the gateway and nothing else is changed. Run 7: https://github.com/linearcombination-ai/DOC/actions/runs/45729013524 (RAN; table produced by `ci/e2e_table.py` from the junit files, artifact `e2e-table`).

| test file (tests/e2e/doc) | collected | TS pass | TS fail | TS skip | DOC pass | DOC fail | DOC skip |
|---|---:|---:|---:|---:|---:|---:|---:|
| test_api.py | 42 | 31 | 0 | 11 | 31 | 0 | 11 |
| test_assemble_by_book.py | 42 | 35 | 0 | 7 | 35 | 0 | 7 |
| test_assemble_by_book_docx.py | 19 | 19 | 0 | 0 | 19 | 0 | 0 |
| test_assemble_by_book_udb.py | 3 | 0 | 0 | 3 | 0 | 0 | 3 |
| test_assemble_by_book_udb_docx.py | 3 | 0 | 0 | 3 | 0 | 0 | 3 |
| test_assemble_by_chapter.py | 37 | 37 | 0 | 0 | 37 | 0 | 0 |
| test_assemble_by_chapter_udb.py | 3 | 0 | 0 | 3 | 0 | 0 | 3 |
| test_assemble_by_verse_book_at_a_time.py | 1 | 0 | 0 | 1 | 0 | 0 | 1 |
| test_assemble_by_verse_chapter_at_a_time.py | 2 | 0 | 0 | 2 | 0 | 0 | 2 |
| test_assembly_by_chapter_sl_sr.py | 3 | 0 | 0 | 3 | 0 | 0 | 3 |
| test_biel_helper.py | 1 | 1 | 0 | 0 | 1 | 0 | 0 |
| test_dynamic_usfm_fixes.py | 306 | 208 | 0 | 98 | 208 | 0 | 98 |
| test_resource_lookup.py | 1 | 1 | 0 | 0 | 1 | 0 | 0 |
| test_send_email.py | 1 | 1 | 0 | 0 | 1 | 0 | 0 |
| test_stream_pdf.py | 1 | 1 | 0 | 0 | 1 | 0 | 0 |
| test_use_prince.py | 1 | 0 | 0 | 1 | 0 | 0 | 1 |
| **total** | **466** | **334** | **0** | **132** | **334** | **0** | **132** |

**Tests whose outcome differs between the stacks: 0.** The 132 skipped are DOC's own `@pytest.mark.skip` and marker skips, plus four modules (`*_udb*`, `test_assembly_by_chapter_sl_sr`) skipped as a whole, once per pytest group (identical on both stacks). Excluded as out of this phase (listed, not hidden): `tests/e2e/stet/test_api_stet_docx.py` (4 tests), `tests/e2e/passages/test_api_passages_docx.py` (1 test), the unmarked-in-CI randomized tests (`randomized`, `english_randomized`, `all`), and DOC's `tests/unit`.

**DOC's Playwright frontend tests** (`frontend/tests/e2e/doc_test_*.ts`, DOC's own `testsDockerfile` image, 4 shards, `retries: 1` as DOC's config has it; STET and Passages files excluded: `stet_test.ts` 1, `stet_test_2.ts` 0, `passages_test.ts` 2, `passages_test_2.ts` 3), same run:

| test file | tests | TS pass | TS fail | TS skip | DOC pass | DOC fail | DOC skip |
|---|---:|---:|---:|---:|---:|---:|---:|
| doc_test_1.ts | 5 | 5 | 0 | 0 | 5 | 0 | 0 |
| doc_test_2.ts | 5 | 2 | 0 | 3 | 2 | 0 | 3 |
| doc_test_3.ts | 4 | 3 | 0 | 1 | 3 | 0 | 1 |
| doc_test_4.ts | 3 | 3 | 0 | 0 | 3 | 0 | 0 |
| doc_test_5.ts | 7 | 1 | 0 | 6 | 1 | 0 | 6 |
| doc_test_6.ts | 2 | 2 | 0 | 0 | 2 | 0 | 0 |
| doc_test_7.ts | 2 | 2 | 0 | 0 | 2 | 0 | 0 |
| **total** | **28** | **18** | **0** | **10** | **18** | **0** | **10** |

**How the gap closed** (every difference, classified; RAN, runs 5 to 7): the 31 passed of P6 became 334 by: DOC's HTML markup and CSS (our look kept as `OUTPUT_LOOK=proposed`); per-chapter and per-book `reg` and Writer repositories; resources resolved by DOC's `resource_lookup_dto` over the Primary catalog; an empty document when nothing resolves (the `fa` language test); DOC's title page strings and localized book names (the two Playwright failures of run 6); and **DOC's on-the-fly USFM repair rules** for the 98 languages it knows to be defective (6 failures of run 6: the repaired text differs from the repository's, so the verses were missing). Environment differences found only because the tests ran in containers: the output volume ownership (runs 2 and 5). Not a difference: both stacks pass all of DOC's tests with the live catalog and content hosts on the day of the run; a content host outage would fail both (a flake shows on both in the same run).

## Memory at rest and under load per container

`docker stats` (RAM used by the container's cgroup, as the engine reports it; RAN in CI). Limits are the compose defaults.

| Container | limit | at rest (after start, nothing requested) | after the smoke test (docx, html, pdf, lookups) | during DOC's API e2e (3 groups pooled, every 5 s): median / peak | during a 24-document burst | during a 10-minute soak at 1 request/s (median / peak) |
|---|---|---|---|---|---|---|
| api | 768 MB | 81 MiB | 322 MiB | 70 / 238 MiB | 84 MiB | 70 / 82 MiB |
| worker | 3 GB | 118 MiB | 327 MiB | 571 / 1,580 MiB | 308 MiB | 401 / 531 MiB |
| gateway | 128 MB | 4 MiB | 6 MiB | 5 / 7 MiB | 5 MiB | 5 / 6 MiB |
| valkey | 256 MB | 5 MiB | 6 MiB | 5 / 6 MiB | 6 MiB | 6 / 8 MiB |
| frontend (DOC's) | 512 MB | 56 MiB | 47 MiB | not sampled | 49 MiB | 51 / 58 MiB |

DOC's own stack in the same e2e run (3 groups pooled; its containers have no limits set): api (gunicorn) 243 / 244 MiB, worker (Celery) **777 median / 1,619 peak**, usfm parser sidecar 50 / 63, Redis 7 / 9, Flower 40 / 41, file server 4 / 7 MiB. Peaks are not simultaneous, so they are not added. **Reading it:** the TS stack's worker peaks about as high as DOC's Celery worker under the same load (WeasyPrint and Prince on whole books dominate both) but sits lower in the median (571 vs 777 MiB); its API is smaller at rest (81 vs 243 MiB, one node process with the light wasm module vs gunicorn with the full parser). The TS api is 322 MiB after the smoke test because the catalog snapshot (21,000 rows) is held in memory once a lookup endpoint has been used; at rest, and while it only takes document requests (burst, soak), it is 70 to 84 MiB. Not measured: memory with a licence-holding Prince, a container actually hitting its memory limit (OOM kill), CPU throttling, swap. The burst column is one `docker stats` sample taken right after the burst, not a peak.
