# COMPARISON: TypeScript shell, P6 (headings identical to the Rust build's file)

Evidence tags: **RAN** (executed here, output in `results/` or named test), **READ** (code or document), **INFERRED** (reasoning only). DOC commit `50de21cd`. Host: 4 cores, 11 GB shared with the other build's worker; heavy jobs ran one at a time, benchmarks gated on host load below 2.5 (the load at each run is in its result file; the soak and one docx-bench ran at 2.1 to 4.5, said where).

## Run it

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

## Core boundary

* Same as P5: three pure ops, JSON in and out (`job_key`, `job_parse`, `job_assemble`), no I/O, no clock. P6 changes: `key_json` moved into `doc-request`; `doc-wasm` has a `jobs` feature so the **API loads a light module (`pkg-api` 782 KB, request keys and book lists) and the worker the full one (`pkg` 2,972 KB)**; a test-only `fault-injection` feature adds `debug_spin`. Rust edited: 3 files (RAN: `git diff`).
* **API memory effect (RAN, `results/api-memory.txt`, 3 runs each, host load 4.5):** light 139 MB at start vs full 144 MB, after normalising a 21,000-row catalog 292 vs 289 MB, after 300 mixed requests 308 vs 310 MB. **The light module saves about 5 MB; the catalog and Node dominate.** Reported as found: "keep the API light" is satisfied by not loading the interleaver, not by a memory win.
* Catalog normalisation runs on the module's thread in both processes (A4, RAN): worst event-loop gap while normalising 21,000 rows **363 ms inline vs 46 ms on the thread** (the 46 is the JSON in and out on the main thread).

## Job runner

BullMQ 6.7.10 on Redis 8.12.0 or Valkey 9.2.0; pipeline function unchanged in shape (`runDocumentJob(deps, data, ctx)`), 1,215 lines of job-runner TypeScript. P6 adds: per-repository **cross-process single flight** (Redis `SET NX PX` lease, renewal every lease/3, fencing number from `INCR`, publish refused when a newer generation exists; falls back to cloning when Redis is unavailable). RAN: two worker processes, empty cache, slow clone: **2 clones for 2 repositories with the lock, 4 without** (`test/repolock.test.ts`); holder SIGKILLed mid-clone: lease expires, the other worker finishes the job; fencing test: a holder that lost its lease is fenced out and adopts the newer copy. Residual window (READ, in `repolock.ts`): check and rename are two steps; both results are complete clones.

## Docx seam

Two modes behind `DocxRunner`, switch `DOCX_MODE`: `process` (one Python process per job) and `persistent` (pool of `docx_server.py`, JSON lines on stdin/stdout, default). Supervision (RAN, `test/docx.pool.test.ts`, 11 tests): same pid across jobs; crash mid-job fails that job transient and the process is replaced; deadline kills the process group and replaces it; abort kills like a deadline; memory ceiling kills mid-job (`docx_worker_memory`, transient); recycle after N jobs; idle crash refilled; N jobs on M workers queue (never more than M processes); `close()` leaves none. Formats: docx, html, pdf (WeasyPrint through one function `pdf_bytes`, the Prince swap point).

**Cost per job, measured through the real pipeline, 30 jobs each, one at a time, different documents (RAN, `results/docx-bench.txt`, host load 4.5 so absolute numbers are pessimistic):**

| mode | end to end median (p95) | docx stage median (p95) | Python generation median |
|---|---|---|---|
| process | 365 ms (921) | 306 ms (848) | 87 ms |
| persistent | **110 ms (537)** | **67 ms (478)** | 64 ms |

The persistent pool removes about 240 ms of interpreter start and imports per job. Soak memory of its processes is below.

## Fault tolerance

All RAN on real processes (Redis or Valkey, git, Python, SMTP server), no fakes; final result per server in `results/suite-redis8.txt` and `results/suite-valkey9.txt` (both listed below under Valkey).

| Fault | Test | Result |
|---|---|---|
| catalog down longer than client retries | `job.fault` | job retries, document produced |
| permanent error (book missing) | `job.fault` | 1 attempt, `{code, retryable:false}` |
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

## Size

`bash tools/loc.sh` (RAN, lines with comments and blanks): Rust core copied from P3b/P4 7,676 (+360 generated tables), Rust entry points 266; TypeScript: contract 238, app/mains/config 683, core wrapper + catalog + acquirer + lookup + util 1,177, job runner 1,215, docx seam 637, core thread 122, clone lock 108, email 160, observability 276; Python seam 323 + generator 769 (stdlib only); tests 3,210 (141 cases in 15 files); tools and scripts 1,197; generated 1,657 + 928. Dependencies added in P6: `nodemailer` (runtime; maintained by the Nodemailer project, 15+ years, MIT-0, v10.3.1) and `smtp-server`, `@types/*` (tests); production tree otherwise as P5 (BullMQ 6.7.10, ioredis 6.0.0, Hono 4.13.21, zod 4.6.7, pino 10.4.0). Rust: 85 crates in the wasm build as P5. `any`: none in TypeScript (grep; the casts `as never` for nodemailer's logger option and `as object` for ioredis' constructor overload are explained in comments); `typing.Any`: none in Python; **mypy 2.4.1 `--strict` clean over 10 Python files** (`scripts/mypy.sh`).

## Build and test cycle times

wasm 2 min 26 s cold (3 modules: +6 s, +4 s), `tsc --noEmit` about 6 s, full suite about 3 to 4 minutes (155 tests, files one at a time), the Redis build about 9 min and Valkey about 4 min (once), `npm ci` 11 s. No build step for TypeScript (Node strips types).

## Weaknesses

1. **The e2e pass rate against DOC's own tests is low (4 of 31 here)**: HTML markup differs by decision (the look), and per-chapter `reg` repositories are not read. These are gaps in this slice, not in the queue.
2. **Light API wasm barely changes memory** (5 MB of 290).
3. Redis (or Valkey) stays a hard dependency; Redis 8's licence needs a decision; Valkey is the permissive alternative and the suite passes on both.
4. At-least-once with an at-most-once email: the one window (process killed between SMTP acceptance and the record) loses the email, never duplicates it.
5. Metrics are Redis-backed: they reset if Redis loses data; every increment is a Redis command (fire and forget).
6. The cross-process lock needs a shared asset directory; its residual fencing window is explained in `repolock.ts`.
7. WeasyPrint memory on whole books is large (360 MB in the pool before the ceiling was raised to 2.5 GB for PDF); a persistent pool shares fonts and caches across jobs (flat in the soak).
8. `npm`/node_modules size and a Node 26 requirement; two runtimes plus Python in an image.
9. Teardown of QueueEvents hangs for 8 s after a Redis stop in tests (bounded in the harness; does not affect the service, which exits by timer).
10. My DOC baseline is impaired (sidecar refused connections during part of the e2e run; no Prince).

## What was not done

* **PDF with Prince**: only WeasyPrint (the engine is one function); `PDF_ENGINE=off` answers the typed 501.
* **Per-chapter `reg` repositories**, STET and Passages routes, `epub` (removed).
* **A clean DOC e2e baseline** (sidecar flakiness) and a pass-rate table on the full tests tree (only `tests/e2e/doc/test_api.py`).
* The soak did not inject faults (the fault suite does); no run above 24 concurrent jobs; no multi-host test of the clone lock; no Prometheus server scraped the endpoint (grammar checked only); no gateway run (no docker).
* Word was not opened on any output; the Open XML validator was not re-run on the P6 docx (generator unchanged from P5, which passed it).
* The optional Effect comparison was not attempted.
* Redis persistence at scale (memory for retained job records) is inferred, not measured.
