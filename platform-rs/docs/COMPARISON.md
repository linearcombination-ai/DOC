# Comparison sheet: Rust shell, P6 (`doc-p6-rs`)

Same headings as the TypeScript build. Evidence tags: **RAN** (executed on this host, output kept under `results/`), **READ** (source or
document read), **INFERRED** (reasoning only). DOC commit `50de21cd3bab5c6f48961da64257118f6f0ead0e`. Host: 4 CPUs, 11.9 GB shared with the other build;
the 1-minute load average is recorded next to every measurement. No docker.

Pinned versions (registry facts READ from crates.io / npm on 2026-11-25, `results/registry-versions.txt`): rustc/cargo 1.100.0; `redis` crate 1.8.3
(latest; 136M downloads); tokio 1.54.1; axum 0.8.9; reqwest 0.13.5; rustls 0.23.46; tokio-rustls 0.26.6; base64 0.23.1; nix 0.31.4 (0.32.0 exists, not
needed); utoipa 6.0.0; tracing 0.1.44; Redis 8.12.0 and Valkey 9.2.0 (latest tags, built from source with `BUILD_TLS=yes`); kitchen v0.1.24 `0090c0be`
(read-only clone); Python 3.12.3 (tests, worker) and 3.13.18 (DOC's venv: mypy 2.4.1, weasyprint); openapi-typescript 7.13.0 (latest 7.x), typescript 5.9.3
(the frontend's `^5.9.3`; npm latest is 7.0.2, not tried); node 18.19.1.

## Run it

* **RAN.** README commands, from a clean copy (`results/readme-run.log`, section at the end): environment, kitchen clone, release build, `stack_up`, then the requests.
* Time to first response from a cold start of the stack (`stack_up`): about 2 s (Redis 0.5 s, mirror, daemon listening). **Time to first document, cold process, fake git:** 0.35 s per-job docx mode, 0.21 s persistent mode (`results/bench-*.json`, load 1.9 to 2.0). With the **real catalog and the real content host, cold, empty cache**: 10.8 s for English ULB + Translation Notes (Jude), docx (`results/real/real-network.json`, load 0.8; includes the clone of `en_tn`, 132 MB). Then 0.31 s for the HTML of the same request, 0.31 s for a new key, 0.054 s for a cache hit.
* Local stack pieces: `scripts/stack.sh` (functions), `scripts/mock_data_api.py` (a data API mirror; SIGUSR1 toggles 503), `scripts/fake_smtp.py`, `scripts/build-servers.sh`.
* Clean release build: 5 m 38 s (section "Build and test cycle times").

## Contract

* **Endpoints (RAN, `contract/openapi.json`, 15 paths):** the seven read endpoints and the four `/findings/...` routes (P4), `POST /documents_docx`, `POST /documents`, `GET /task_status/{id}`, `GET /health/status`, `GET /metrics`. One definition per type in `doc-contract` (serde for the bytes, utoipa for the document); the job endpoints' request and responses are in `crates/doc-contract/src/jobs.rs`.
* **Legacy strings (RAN, golden e2e from P5, still green):** `PENDING`, `Receiving request`, `Locating assets`, `Provisioning asset files`, `Parsing asset files`, `Assembling content`, `Converting to Docx`, plus DOC's `Sending email` and `Converting to PDF`, then `{"state":"SUCCESS","result":"<key>"}` or `{"state":"FAILURE","result":{code,message,retryable}}` (the result is added to FAILURE; the legacy body has none). An unknown id is `PENDING`, as with Celery. A retrying job keeps its last progress string, so the UI never sees a false FAILURE (RAN).
* **Findings:** `/findings/...` routes plus `X-Doc-Findings: N` (P4, RAN); each document has `<key>.findings.json` next to it; email problems and "no verses" notices are appended to it.
* **Typed errors (RAN):** 422 FastAPI-shaped `detail` list; 413 over `DOC_JOBD_MAX_BODY_BYTES`; 503 `queue_unavailable` and `disk_space_low` with `Retry-After` and `retryable:true`; 501 `pdf_not_implemented` (when no PDF engine is configured).
* **Request shapes (RAN, `results/shapes.json`, test `the_request_shapes_the_ui_can_post...`):** 17 shapes, each as docx and as HTML, all through the same pipeline: ULB alone; ULB+TN by book (lbo) and by chapter (blo); print layout; ULB+TN+BC; TN only; condensed TN only; BC only; ULB+TW as links, with definitions included, and with only the words used (`tw_mode`, a new optional field: the captain's "include or link"); Spanish ULB+TQ; two languages by book; two languages by chapter in two columns; the UI's body exactly as sent (with `generate_epub`, `use_prince`, and a field DOC's model does not have); by-verse (lvo, bvo; the backend capability the UI hides). 34 of 34 outputs valid (docx: valid package by `check_docx.py`; HTML: closing tags, verse markup). The negative control (a resource the catalog lacks) is a typed permanent failure `resource_not_found`. **Not verified:** that the *content* of each is what the content team wants; nobody opened any output in Word.
* **HTML output (RAN):** DOC's `compose_document`, the "no verses" notice, DOC's header templates (rendered by a 40-line renderer of the three Jinja constructs they use, from DOC's own files), `enclose_html_content`; chapters in DOC's own markup (`div.chapter`, `span.verse`). Deliberate differences: titles are HTML-escaped (DOC does not escape); the `<script>`-free header is DOC's. **PDF (stretch, RAN):** `POST /documents` with `generate_pdf=true` runs the HTML then `python/pdf_worker.py` with WeasyPrint (default) or Prince (`DOC_PDF_ENGINE`), same supervision as docx; test `pdf_through_the_python_seam_and_a_hung_engine_is_killed` (a hung engine killed at its deadline, the retry produced a valid PDF, emailed once). **Prince is not exercised** (no binary here); `use_prince` in the request is ignored when the engine is WeasyPrint.
* **Email (RAN):** after the document, idempotent per task (section "Fault tolerance").
* **Frontend types (G, RAN, `results/frontend-types.log`):** `contract/api.d.ts` generated by openapi-typescript 7.13.0 (0.18 s). `contract/frontend-dropin.ts` states how `frontend/src` calls the endpoints (store types copied from `SettingsStore.ts:8-27`, the request built as `GenerateDocument.svelte:78-100`, `poll()`, the error path) and compiles under `tsc --strict` 5.9.3 without changing the frontend. **Two mismatches** (`contract/frontend-mismatch.ts`, fails on purpose): (1) `assemblyStrategyKindStore` is typed `Writable<string>` (`SettingsStore.ts:9`), the contract's field is the union `'lbo'|'blo'|'lvo'|'bvo'|'stet'` (TS2322); (2) the UI sends `show_rg_chapter_commentary` (`GenerateDocument.svelte:99`), a field DOC's `DocumentRequest` does not have (pydantic ignores it; an object literal typed to the contract rejects it, TS2561). The UI's `res.json()` is `any`, so the poll and error paths type-check trivially. The health response gained fields; a client reading `[0].status` and `[1]` is unaffected.
* **DOC's own API e2e tests (F, RAN, `results/e2e-*.log`):** `tests/e2e/doc/test_api.py` (42 tests, 11 skipped by DOC), unmodified, pointed at a service by the plugin `scripts/e2e/e2e_remote.py` (replaces `fastapi.testclient.TestClient` with an httpx client; the tests read the output directory). Real catalog and real git for both. Baseline: DOC's FastAPI + Celery (2 workers) + the .NET sidecar + Redis, started by `scripts/e2e/baseline.sh` (PATH includes the venv so `weasyprint` resolves; **no Prince binary on this host**).

| | passed | failed | skipped by DOC | wall |
|---|---|---|---|---|
| DOC FastAPI + Celery (baseline) | 23 | 8 | 11 | 3 m 51 s |
| doc-jobd (this build) | 18 | 13 | 11 | 8 m 32 s (clones cold) |

Every difference, classified (31 tests; 0 tests fail in both):

| Class | Tests | Cause |
|---|---|---|
| **Baseline fails, doc-jobd passes (8)** | the fr/pt-br `..._1c_c` and `1c` tests with TQ/TN | environmental on the baseline: DOC shells out to `prince` (`use_prince` default true in the UI body) and there is none here (`FileNotFoundError: 'prince'`); doc-jobd ignores `use_prince` and uses WeasyPrint. Not a regression in either; a Prince-equipped DOC would pass these. |
| **doc-jobd fails, baseline passes (13)** | | |
| A. resources outside the Primary catalog (7 tests: en TQ/TW with sw; fa ULB+TN) | `...en_tq...sw_tw...` x6, `fa_ulb_tn` | The pipeline locates resources from the *Primary* catalog query (as P4's read endpoints do); `en_tq`/`en_tw` for these requests resolve through DOC's other lookup path (many `en_tq` repositories exist with status Active, not Primary). Error `resource_not_found: no repository for en tq`. **A real gap**, not fixed. |
| B. Writer per-book repositories (5 tests: bys, bjz) | `bys_*`, `bjz_*` | `book_not_found: has no USFM file for col/eph`. I added "prefer the repository named for the book" and a `.txt`-USFM fallback to the locator; the five still fail (the repository chosen or its file layout is still not DOC's). **A real gap**, not fixed. |
| C. TN chapter intro (counted inside A's list: two sw/en TN-by-chapter tests) | | `resource_unreadable: FileNotFoundError tit/01/intro.md`: the core asks for a chapter intro file DOC tolerates when absent. **A core gap** (P3b reader), not fixed. |

So the pass rate against the baseline is **18 of 31 (58 %) against 23 of 31 (74 %)**, and the baseline's own 8 failures are a missing Prince binary. The slice that works end to end is the interleaved ULB + TN/TQ/TW/BC for languages whose resources are in the primary catalog. **Not run:** the other test files (`test_assemble_by_*`, about 9,000 lines), `tests/e2e/stet` and `passages`, and the randomized suites; the PDF-specific tests were run (they pass through WeasyPrint) but the Prince ones cannot be.

## Core boundary

* **RAN:** the core (P3b crates, kitchen adapter) is called in a **child process of the same binary** (`doc-jobd core-stage IN OUT`, JSON in and out; the child reads repository directories itself), under a deadline and the job's cancel signal; the whole process group is killed and reaped on every exit path. Tests: a hung core is killed at the stage deadline (pid gone) and the job retried (`a_hung_core_is_killed_at_the_stage_deadline_and_the_job_retried`); a core that crashes twice is two transient failures, then success.
* Cost: the child adds about 10 ms per job (core stage 14 ms against 4 + 1 ms inline in P5; RAN, `results/bench-*.json`). `DOC_CORE_MODE=inline` keeps the P5 behaviour for tests and the Redis-less mode (not cancellable).
* One status string is approximate: the child does parse and assemble together, so `Assembling content` is published right after it returns (the legacy order is kept; the label is not exact).
* Catalog normalisation (parse, index, copy of 20k rows, and the adapter's validation) runs on the blocking pool. **Found by the test:** the adapter's own validation parsed the 5 MB body on a runtime thread; fixed. Test `catalog_normalisation_runs_off_the_runtime_threads` (one runtime thread, a 120,000-row catalog whose inline normalisation takes 6.2 s): the longest gap of a 5 ms ticker was **20 ms**; before the fix it was 5,976 ms (RAN).
* No `unsafe` (crate lint forbid), no `unwrap`/`expect`/`panic` in library code (workspace lint deny; tests allow them).

## Job runner

* Hand-written on Redis Streams plus Lua (the P5 conclusion kept): `redis_store.rs` 479 lines, 10 Lua scripts (about 190 lines), `runner.rs` 371, `mem_store.rs` 334 (the Redis-less store behind the same trait).
* **apalis-redis re-evaluated (RAN, `results/apalis-recheck.txt`, `scripts/apalis_recheck.sh`):** the latest release is still 1.2.0 (2026-11-22; 1.1.1, 1.1.0, 1.0.0 before). Result unchanged: with the default config two pushes of one key store **no idempotency key at all** (so no de-duplication); with `idempotency_ttl = 3600` the key is still held after the job finished (ttl 3596 s after completion: a failed or finished request cannot be re-queued until it expires); orphan recovery works (1.4 s) only because the heartbeat was set to 1 s x 2 (the default is 30 s x 10); no per-job progress string. Verdict: not adopted; the four wrappers it would need are the runner.
* **Cross-process single flight (A, RAN):** a Redis lock per repository (`SET`-style with a lease, renewal every lease/3, an `INCR` fencing token per key, compare-and-delete release) behind the `RepoLock` port; the acquirer takes it inside its refresh, re-checks the cache after getting it, and verifies it still holds the lock before publishing the clone. Tests: **6 jobs on 2 worker processes clone each of the 2 repositories exactly once (2 `git` invocations)**; the control with the lock off clones more than once; a killed lock holder is replaced when its lease expires (4 invocations, document valid, fence token grew to 2); a stale holder's release does not remove the new holder's lock (unit). If Redis cannot grant the lock the clone proceeds without it (a duplicate clone is only wasted work).
* **At-least-once, stated:** a job can run twice (SIGKILL, long outage); safety rests on the output name being the key, atomic rename, fencing of every state change, the cache check, and the email record.
* Retries: jittered backoff (full jitter, floor 100 ms), transient versus permanent codes (RUNBOOK section 5), `max_attempts` 8 by default, a poison job (delivered more than max + 2 times) is stopped with `worker_lost_repeatedly`.
* Disk: below `DOC_JOBD_MIN_FREE_MB` new requests get a typed 503 and running jobs fail transiently (RAN); dead scratch removed at start and by the janitor; outputs expire after `ASSET_CACHING_PERIOD` + `DOC_JOBD_OUTPUT_GRACE_MS` (RAN, test with old and fresh files, non-output files untouched).

## Docx seam

* Two modes behind one switch (`DOC_DOCX_MODE=per_job|persistent`), one job format (`DocxJob` v2, with the request id): per-job = a `python3 -I docx_worker.py --job FILE` process per document (P5); persistent = a pool of `--serve` workers (JSON lines on stdin, answers and per-job `rss_kb`/`jobs_done` on stdout, JSON logs on stderr).
* Pool supervision (RAN, four tests): a crash mid-job (SIGSEGV) is detected by the closed pipe, the worker replaced and the job retried; a hung worker plus its grandchild are killed with the process group at the per-job deadline and replaced; a worker is recycled after N jobs (`restarts.recycle >= 2` for 5 jobs with N = 2) and when its resident size passes the ceiling (30 MB leaked per job, ceiling 70 MB: retired); metrics and `/health/status` show the restarts by reason.
* **Measured cost per job (RAN, release build, fake git, load 1.9 to 2.0, `results/bench-per_job.json`, `bench-persistent.json`, `seam-cost.json`):**

| | per-job process | persistent pool |
|---|---|---|
| docx stage wall, p50 | 155 ms (163 ms in the test) | 25 ms (20 ms in the test) |
| of which the generator itself | 18 ms | 21 ms |
| whole request, warm, p50 / p95 | 0.315 / 0.400 s | 0.160 / 0.235 s |
| cold process, first request | 0.352 s | 0.213 s |
| worker memory | 24 MB per job process | one resident worker about 24 MB, retired by the ceiling |

The pool halves a warm request. With DOC's python-docx path behind the seam (P5: 1.4 s import) the saving would be larger (INFERRED).
* Python side (H, RAN): `mypy --strict` clean over `python/` and `scripts/` (16 files, with `MYPYPATH=../backend` for the scripts that import DOC), no `typing.Any`. DOC's own gate (`mypy.ini`) scans `backend/doc`, `backend/stet`, `backend/passages` and `tests`; this directory is not in it and passes the stricter flags anyway.

## Fault tolerance

Every fault test, all RAN, against Redis 8.12.0 **and** Valkey 9.2.0 (`results/suite-redis.log`, `suite-valkey.log`: 100 passed, 0 failed each, whole workspace; the 42 integration tests of `doc-jobd` are the ones below). Redis exits 0 in 4 m 06 s cold, Valkey in 1 m 36 s warm.

| Fault | Result |
|---|---|
| Data API 500, then recovering | job waited out (6 attempts), then SUCCESS (P5 `catalog_outage_is_waited_out_not_failed`) |
| Clone failing 3 times | 3 attempts, waits inside the jitter windows, SUCCESS |
| Clone hanging | killed at the clone timeout (process group), job completes once git recovers |
| Docx worker crash, exit 3, truncated output | transient, retried, SUCCESS; the truncated file is never placed |
| Docx worker rejects the job (exit 2) | permanent `docx_input_rejected`, 1 attempt |
| Docx worker hangs with a grandchild | both gone at the deadline; retry SUCCESS |
| Per-attempt ceiling exceeded | worker killed, retry SUCCESS |
| **Core child hangs** | killed at the stage deadline, pid gone, retry SUCCESS |
| **Core child crashes twice** | two transient failures, SUCCESS on attempt 3 |
| **Pool worker crashes / hangs / leaks / ages** | replaced; job SUCCESS; restarts counted by reason |
| Worker SIGKILL mid-job | orphaned Python process leaves in 0.24 s; second daemon completes on attempt 2, 2.2 s after the kill; one output |
| **Lock holder SIGKILL** | new holder takes over after the lease; 4 clones in all; document valid |
| **Two worker processes, one repository** | exactly one clone |
| Redis restart mid-run (AOF everysec) | job finishes on attempt 1, queue survives, API typed 503 while down |
| **Redis data lost (no persistence)** | service keeps running and becomes ready again; the lost job's id answers PENDING; a resubmission works |
| 8 identical simultaneous POSTs | 1 task id, 7 coalesced, 1 run |
| SIGTERM with a running job / past the drain deadline | exit 0 after the job / job released without counting an attempt |
| **Email twice** | see below |
| **Redis password wrong / missing, TLS wrong CA, plain text to a TLS port** | refused (`redis_password_and_tls_are_supported`); with the right ones the whole service runs over TLS and the password never appears in the log |
| Oversized body / bad language code / 101 resources / disk full | 413 / 422 / 422 / typed 503 |
| Bad configuration (8 problems at once) | all eight named, exit 2, nothing started |
| Hung PDF engine | killed at its deadline, retry produced the PDF |

**Email (RAN, five tests, a fake SMTP server):** (1) normal: one message with the docx attached and a `Message-ID` derived from the task id; (2) the server refuses RCPT twice (450): the job retries the step and exactly **one** message goes out; (3) the worker is killed (SIGABRT hook) right after the server accepted the message, both before and after the store record was written: the redelivered job sends **nothing more** (1 message in all; in the first window the claim is in doubt and the document's findings say `email_in_doubt`); (4) the worker dies between the claim and the send: **0** messages, `email_in_doubt`, document delivered; (5) the server reads the whole message and closes without the final 250: ambiguous, not retried, `email_in_doubt`. Decision recorded: **at most once** (never twice) at the price of a possible lost email that is reported, because SMTP cannot make the last step exactly-once. A second *request* for the same document is a second email (the key is the task id).

## Observability

* **One structured line per stage and per attempt (RAN):** `attempt started` / `stage finished` (locate, provision, core, docx, pdf) / `attempt succeeded; job done` or `attempt failed (transient)` / `attempt failed; job failed`, all with `request_id`, `task_id`, `attempt`, `key`, duration, result. The request id comes from `X-Request-Id` (or is made, validated to 64 safe characters, and echoed in the response header), is stored with the job, appears on the worker's lines, and is passed to the Python process, whose JSON log lines carry it and are relayed into the same stream (test `the_request_id_reaches_the_worker_and_the_python_process`, both docx modes).
* **`GET /metrics` (RAN):** Prometheus text 0.0.4, validated by a strict parser in the test: queue depth by state, oldest waiting age, job attempts, jobs by outcome, failures by code and kind, stage duration histograms, stalled recoveries, repository clones, cache hits (document and repository), catalog refreshes and failures and last success, email outcomes, pool restarts by reason, worker up, workers alive, pool workers, store up. Counters live in Redis, so any API node reports the cluster; every family is present at zero. `doc_store_up 0` when Redis is down (the scrape still answers).
* **`GET /health/status` (RAN):** DOC's `[{"status":"ok"}, 200]` shape with `ready`, `problems`, `redis`, `queue` (depth, `lag_s`), `processes` (alive, docx pool, last catalog fetch), `catalog`, `disk`. With Redis down it still answers 200 (`status ok`) and says `ready:false` with the reason.
* RUNBOOK: symptoms, every log line, drain, replay (`scripts/replay.sh`), Redis loss, what each metric alarms on.

## Performance

All RAN on the release build (`target-rel-default`), fake git over fixtures unless stated, load recorded.

* **Latency (load 1.9 / 2.0, `results/bench-*.json`):** warm sequential request p50 0.315 s (per-job docx) and 0.160 s (persistent); p95 0.40 / 0.235 s; cache hit p50 6.2 ms; cold process first request 0.352 / 0.213 s; stage p50: locate 0, provision 0 (cached), core 14 ms, docx 155 / 25 ms. 4 concurrent distinct jobs: 0.64 / 0.51 s wall. **Real network, cold, empty cache:** 10.8 s (see "Run it").
* **Memory:** the service process is 14 MB idle, 16 MB after 20 distinct jobs (RSS); with 4 persistent docx workers and transient core children the whole tree peaked at 83 MB in the soak (2 daemons, 8 jobs at once) and 166 MB at its largest sample; Redis 14 to 34 MB. DOC's Celery worker tree is about 426 MB (P5, RAN, idle).
* **Soak (F, RAN, `results/soak/`, 31 minutes, 2 daemon processes sharing Redis and the clone cache, 4 submitter threads, persistent pool, mixed requests: 10 shapes, 70 % docx / 30 % HTML, random option bits, one repeat in 7 (cache hits), a burst of 5 identical every 25th, outputs expiring after 3 minutes):** **16,780 jobs, 0 failed, 0 HTTP errors**, 2,144 requests coalesced; latency p50 0.156 s, p95 0.373 s, p99 0.679 s, max 3.05 s; both daemons exited 0 on SIGTERM. Host load 1.6 at start, mean about 2.4, peak 4.1 (the other worker's builds; the soak itself was never started above 3). **Memory: no leak.** The daemons' resident size went from 30.8 MB to 40.7 MB in the first minutes (pool and caches warming) and then stayed flat: mean 39.4 MB in the first third after 5 minutes, 40.2 MB in the last third; linear slope after minute 5: **+39 kB per minute** (about 1 MB over the remaining 26 minutes). Children (pool workers, core children) stayed between 50 and 104 MB. Output files stayed bounded (the janitor held them under 1,000). `results/soak/soak.csv` has every 5-second sample.
* **20 or more concurrent jobs (RAN, `results/burst/burst.json`, load 2.75):** 24 distinct docx jobs submitted at once across 2 processes (8 workers each): all 24 SUCCESS in 1.27 s wall (latency p50 1.10 s); 24 HTML jobs 0.72 s; peak memory of daemons plus children 370 MB. (The third burst, "mixed", was served from cache and says little.)
* **DOC's side:** the baseline e2e run took 3 m 51 s for 31 tests against 8 m 32 s here, but this build cloned the repositories cold over the network and failed 13 tests quickly or late; the two numbers are not a throughput comparison (INFERRED).
* **Not measured:** PSS memory; load above 3 for the benchmarks; a soak against the real network; DOC's FastAPI throughput on the same request mix.

## Size

Lines (RAN, `wc -l`; Rust `src` unless noted): doc-jobs **4,462** (runner 371, store trait 115, redis_store 479, mem_store 334, lock 175, pipeline 490, core_stage 232, seam 247, pool 276, pdf 135, email 365, html 163, metrics 253, config 328, api 211, output 156, limits 63, model 125, retry 53) plus 10 Lua scripts (about 190); doc-jobd 354; the P4 shell crates (contract 622, lookup 1,899, app 315, catalog 302, acquire 544, shell 700, ports 251, flight 143, kitchen 60); the P3b core (model 1,142, request 583, interleave 1,731, md 1,668, canon 1,174). Tests: doc-jobd integration 2,076 (`faults.rs` 497, `p6.rs` 815, `support/` 700+), doc-shell 642 + responsive. Python: docx_worker 250, pdf_worker 117, the docx generator 770, scripts about 1,300. P5 to P6 added about 2,400 lines to doc-jobs and 1,700 of tests.

Dependencies (READ, `Cargo.lock`): about 290 crates for `doc-jobd` (P5: 283; P6 added `rustls`, `tokio-rustls`, `rustls-native-certs`, `rustls-pki-types`, `base64`, all already in the tree through reqwest/redis, so no new crate versions to vet beyond feature flags). Maturity (crates.io, READ): `redis` 1.8.3, 136M downloads, 132 releases since 2014; `tokio` 1.54 (1.2 billion downloads); `axum` 0.8.9 (620M); `rustls` 0.23.46 (1.17 billion); `reqwest` 0.13.5; `nix` 0.31.4 (1.0 billion; used only for `killpg` and `statvfs`); `utoipa` 6.0.0 (67M, youngest of the set). Hand-written and owned by the team: the runner, the Lua, the SMTP client (about 300 lines: chosen so each failure phase is known), the Jinja-subset renderer (40 lines).

## Build and test cycle times

* **Clean release build: 5 m 38 s** wall (user 595 s, cargo's peak RSS 688 MB, 2 jobs, load about 2; P5 recorded 11 minutes on a loaded host) (`results/release-build-default.log`). Binary 23 MB. **Minimising:** a profile without LTO and 16 codegen units took 5 m 56 s on a clean target (`results/fast-build.log`): no gain, so it was not kept; thin LTO is cheap here, the time is the 290 crates. Incremental release after editing the pipeline: **2 m 11 s** (`results/release-incremental.txt`). No faster linker (mold, lld) is installed here; not tried.
* Clean debug test build of the workspace: 4 m 29 s (baseline, load 1 to 6). Incremental debug build after a change in `doc-jobs`: 15 to 60 s.
* Full test run: **100 tests, 1 m 36 s warm** (Valkey run) to 4 m 06 s (Redis run that included recompiling); the 15 P5 fault tests take about 25 s, the 25 P6 integration tests about 50 s.
* Redis 8.12.0 from source: about 9 minutes (the LTO link dominates); Valkey 9.2.0: about 6 minutes.

## Weaknesses

* **Parity gaps found by DOC's own tests (above):** resources outside the primary catalog, Writer per-book repositories, TN chapter intros. These are in the P5/P6 pipeline and the P3b reader, not in the runner; they are the honest reason for 18/31.
* More hand-written code to own: about 1,700 lines of runner, store, lock and Lua, plus the SMTP client. 100 tests, but all written by the same author.
* Email is at most once, not exactly once (stated above).
* The core child costs about 10 ms per job and forces `Assembling content` to be published after it returns.
* Prince is untested; WeasyPrint output was only checked as a valid PDF, not visually.
* A pooled docx worker leaks nothing measurable in 31 minutes (recycled every 300 jobs), but the generator is the stdlib one; DOC's python-docx path (large imports, 131 MB) behind the same seam was not measured.
* Redis without persistence loses queued state (documented, tested); Sentinel/Cluster not tried (the Lua touches keys outside `KEYS`; INFERRED that cluster mode would need hash tags).
* The soak ran against fixtures, not the real network; real-network evidence is one cold run.
* `DOC_TEMPLATES_DIR` ties the service to DOC's template files; the HTML header is DOC's, so a changed template changes the output.

## What was not done

* Opening any output in Word, or visually comparing HTML/PDF with DOC's (the captain's check).
* The e2e table covers `test_api.py` only (31 runnable tests); other e2e files, STET and Passages, and the randomized suites were not run. The three parity gaps above are not fixed.
* Prince (no binary); PDF page-level checks.
* Docker, Sentinel/Cluster, a gateway deployment (designed in RUNBOOK section 9, not built).
* A soak with real-network clones; benchmarks above load 3; PSS memory; FastAPI throughput.
* The TypeScript build was read only through its checkpoint (`data/doc-p6-ts/checkpoint.md`, 05:21Z): nothing in it applied to this build that was not already planned.
