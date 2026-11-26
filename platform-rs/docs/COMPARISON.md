# Comparison sheet: Rust shell, P7 (`doc-full-rs`): the full system in containers

Same headings as the TypeScript build's file (the P6 headings, then three new ones: **Docker and CI**, **DOC e2e pass table**, **Memory at rest and under load per container**).
Evidence tags: **RAN** (executed, output kept under `results/` or named), **READ** (source or document read), **INFERRED** (reasoning only). DOC commit
`50de21cd3bab5c6f48961da64257118f6f0ead0e`. Two places ran things: the authoring host (4 CPUs, 11.9 GB, **no container engine ever run on it**; the P6 numbers below were measured there) and
**GitHub Actions on the fork** `linearcombination-ai/DOC` (ubuntu-24.04 runners, Docker 28.0.4, Compose v2.38.2; every container number is from there, with the run URL).
Sections written for P6 and not touched by P7 keep their P6 numbers and say so; the P6 evidence files they cite (`results/...`) are in `data/doc-p6-rs/prototype/results/`, not in this copy (this copy's `results/` holds the CI evidence of P7).

Pinned versions (registry facts READ from crates.io / npm on 2026-11-25, `results/registry-versions.txt`): rustc/cargo 1.100.0; `redis` crate 1.8.3
(latest; 136M downloads); tokio 1.54.1; axum 0.8.9; reqwest 0.13.5; rustls 0.23.46; tokio-rustls 0.26.6; base64 0.23.1; nix 0.31.4 (0.32.0 exists, not
needed); utoipa 6.0.0; tracing 0.1.44; Redis 8.12.0 and Valkey 9.2.0 (latest tags, built from source with `BUILD_TLS=yes`); kitchen v0.1.24 `0090c0be`
(read-only clone); Python 3.12.3 (tests, worker) and 3.13.18 (DOC's venv: mypy 2.4.1, weasyprint); openapi-typescript 7.13.0 (latest 7.x), typescript 5.9.3
(the frontend's `^5.9.3`; npm latest is 7.0.2, not tried); node 18.19.1.

## Run it

* **Containers (P7, RAN in CI):** `cd platform-rs && docker compose up --build`, then http://localhost:8001 (DOC's UI), API `:5005`, files `:8089`. In CI, with the images loaded, **every service is healthy 26 s after `docker compose up -d`** (run 2: `up_seconds=26`), then the smoke test (catalog lookups, docx 2.2 s, html 1.1 s, PDF through WeasyPrint 2.2 s, PDF through Prince 1.1 s with no fallback finding, the UI) takes 17 s. A first `docker compose up --build` on a 4 vCPU runner builds the four images in parallel in about 5 minutes (core 279 s cold, the slowest); on the captain's machine: INFERRED, not measured. `README.md` is the shortest path.
* **Without containers (P6, RAN on the authoring host).** README commands, from a clean copy (`results/readme-run.log`, section at the end): environment, kitchen clone, release build, `stack_up`, then the requests.
* Time to first response from a cold start of the stack (`stack_up`): about 2 s (Redis 0.5 s, mirror, daemon listening). **Time to first document, cold process, fake git:** 0.35 s per-job docx mode, 0.21 s persistent mode (`results/bench-*.json`, load 1.9 to 2.0). With the **real catalog and the real content host, cold, empty cache**: 10.8 s for English ULB + Translation Notes (Jude), docx (`results/real/real-network.json`, load 0.8; includes the clone of `en_tn`, 132 MB). Then 0.31 s for the HTML of the same request, 0.31 s for a new key, 0.054 s for a cache hit.
* Local stack pieces: `scripts/stack.sh` (functions), `scripts/mock_data_api.py` (a data API mirror; SIGUSR1 toggles 503), `scripts/fake_smtp.py`, `scripts/build-servers.sh`.
* Clean release build: 5 m 38 s (section "Build and test cycle times").

## Contract

* **Endpoints (RAN, `contract/openapi.json`, 15 paths):** the seven read endpoints and the four `/findings/...` routes (P4), `POST /documents_docx`, `POST /documents`, `GET /task_status/{id}`, `GET /health/status`, `GET /metrics`. One definition per type in `doc-contract` (serde for the bytes, utoipa for the document); the job endpoints' request and responses are in `crates/doc-contract/src/jobs.rs`.
* **Legacy strings (RAN, golden e2e from P5, still green):** `PENDING`, `Receiving request`, `Locating assets`, `Provisioning asset files`, `Parsing asset files`, `Assembling content`, `Converting to Docx`, plus DOC's `Sending email` and `Converting to PDF`, then `{"state":"SUCCESS","result":"<key>"}` or `{"state":"FAILURE","result":{code,message,retryable}}` (the result is added to FAILURE; the legacy body has none). An unknown id is `PENDING`, as with Celery. A retrying job keeps its last progress string, so the UI never sees a false FAILURE (RAN).
* **Findings:** `/findings/...` routes plus `X-Doc-Findings: N` (P4, RAN); each document has `<key>.findings.json` next to it; email problems and "no verses" notices are appended to it.
* **Typed errors (RAN):** 422 FastAPI-shaped `detail` list; 413 over `DOC_JOBD_MAX_BODY_BYTES`; 503 `queue_unavailable` and `disk_space_low` with `Retry-After` and `retryable:true`; 501 `pdf_not_implemented` (when the API role has no PDF engine configured: in the stack both roles are given `DOC_PDF_ENGINE=remote`).
* **Request shapes (RAN, `results/shapes.json`, test `the_request_shapes_the_ui_can_post...`):** 17 shapes, each as docx and as HTML, all through the same pipeline: ULB alone; ULB+TN by book (lbo) and by chapter (blo); print layout; ULB+TN+BC; TN only; condensed TN only; BC only; ULB+TW as links, with definitions included, and with only the words used (`tw_mode`, a new optional field: the captain's "include or link"); Spanish ULB+TQ; two languages by book; two languages by chapter in two columns; the UI's body exactly as sent (with `generate_epub`, `use_prince`, and a field DOC's model does not have); by-verse (lvo, bvo; the backend capability the UI hides). 34 of 34 outputs valid (docx: valid package by `check_docx.py`; HTML: closing tags, verse markup). The negative control (a resource the catalog lacks) was a typed permanent failure `resource_not_found` in P6; in P7 it follows DOC (the request is left out, the document is built, `resource_not_in_catalog` is in the findings) and only "every resolved resource is a book its repository lacks" is permanent (`resource_unavailable`). **Not verified:** that the *content* of each is what the content team wants; nobody opened any output in Word.
* **HTML output (RAN):** DOC's `compose_document`, the "no verses" notice, DOC's header templates (rendered by a 40-line renderer of the three Jinja constructs they use, from DOC's own files), `enclose_html_content`; chapters in DOC's own markup (`div.chapter`, `span.verse`). Deliberate differences: titles are HTML-escaped (DOC does not escape); the `<script>`-free header is DOC's. **PDF (RAN):** `POST /documents` with `generate_pdf=true` runs the HTML, then the Python service converts it with Prince when the request says `use_prince` (WeasyPrint otherwise, and as the fallback when Prince is missing, fails or hangs, with `pdf_engine_fallback` in the findings); test `pdf_through_the_python_seam_and_a_hung_engine_is_killed` (a hung engine killed at its deadline, the retry produced a valid PDF, emailed once). **Prince ran in CI** (the `.deb` DOC's Dockerfile installs, version 16.1, no licence file): the smoke test's PDF with `use_prince` carries no fallback finding, and DOC's `test_use_prince` / `test_stream_pdf` PDF tests are in the 334.
* **Email (RAN):** after the document, idempotent per task (section "Fault tolerance").
* **Frontend types (G, RAN, `results/frontend-types.log`):** `contract/api.d.ts` generated by openapi-typescript 7.13.0 (0.18 s). `contract/frontend-dropin.ts` states how `frontend/src` calls the endpoints (store types copied from `SettingsStore.ts:8-27`, the request built as `GenerateDocument.svelte:78-100`, `poll()`, the error path) and compiles under `tsc --strict` 5.9.3 without changing the frontend. **Two mismatches** (`contract/frontend-mismatch.ts`, fails on purpose): (1) `assemblyStrategyKindStore` is typed `Writable<string>` (`SettingsStore.ts:9`), the contract's field is the union `'lbo'|'blo'|'lvo'|'bvo'|'stet'` (TS2322); (2) the UI sends `show_rg_chapter_commentary` (`GenerateDocument.svelte:99`), a field DOC's `DocumentRequest` does not have (pydantic ignores it; an object literal typed to the contract rejects it, TS2561). The UI's `res.json()` is `any`, so the poll and error paths type-check trivially. The health response gained fields; a client reading `[0].status` and `[1]` is unaffected.
* **DOC's own API e2e tests (P7, RAN in CI, run 2 https://github.com/linearcombination-ai/DOC/actions/runs/45740243168):** `tests/e2e/doc`, unmodified, in DOC's own image, pointed at the gateway by the plugin `e2e/e2e_remote.py`; DOC's CI marker expression; 3 groups; the same tests on DOC's FastAPI + Celery + sidecar stack in the same run. **334 passed / 0 failed / 132 skipped on both stacks; 0 tests differ.** The per-file table is in "DOC e2e pass table". In P6 the same selection of `test_api.py` was 18 of 31 (DOC's stack 23 of 31 on a host without Prince); P7 closed the three gaps found then (resources outside the primary catalog, Writer per-book repositories, TN chapter intros) by porting DOC's `resource_lookup_dto`, its USFM file discovery and Writer stitching, and tolerating absent files, and added DOC's on-the-fly USFM repairs (98 languages), title page strings and localized book names.
* **DOC's Playwright tests (P7, RAN in CI, run 3 https://github.com/linearcombination-ai/DOC/actions/runs/45741220207):** `frontend/tests/e2e/doc_test_*.ts` against DOC's own UI image, 4 shards: **17 passed / 1 failed / 10 skipped; DOC's own stack 18 / 0 / 10.** The one difference is a captain decision, not a defect: the test waits for the Amharic book name `2 ኛ ቆሮንቶስ` (DOC's normalisation inserts the space); the standing rule keeps the name as the translators wrote it (`2ኛ ቆሮንቶስ`). With `DOC_BOOK_NAME_RULE=doc` that test passes (proof job, run 4, see "DOC e2e pass table").

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

* **P7: a third mode, `remote` (RAN in CI).** In the container stack the worker calls the Python service in its own container over HTTP (`POST /v1/docx`, `POST /v1/pdf`; job files on the shared volume; `python/seam_server.py`, 374 lines, mypy `--strict` clean). The service enforces the deadline (the conversion runs in its own process group and is killed at the deadline), caps concurrency (`PYWORKER_CONCURRENCY`), drains on SIGTERM, and falls back from Prince to WeasyPrint for one document when Prince is missing, fails or hangs (the finding `pdf_engine_fallback` is written next to the document). The client maps every failure to the transient or permanent codes of the child-process modes. What a separate container costs: one HTTP round trip per document (not isolated as a number); what it buys: the Python process and its memory are limited and restarted by the engine on their own (a crash of the Python service mid-PDF was recovered in 8 s in CI, see "Fault tolerance").
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

**P7: the same faults in containers (RAN in CI, the `ops` job, `ci/ops.sh`; run 5 https://github.com/linearcombination-ai/DOC/actions/runs/45744949895, "OPS: all scenarios passed"; evidence in `results/run5-ops/`).** The P6 rows above stay as measured on the host.

| Scenario (in the compose stack) | Result |
|---|---|
| Limits applied | memory limits valkey 256 MB, pyworker 3 GB, api 768 MB, worker 1 GB, gateway 128 MB, frontend 384 MB; restart `unless-stopped` on all six; stop timeouts 20 / 150 / 30 / 180 s (`docker inspect`) |
| SIGTERM to the worker with a job in flight (`docker compose stop`) | exit 0 after 11 s, log line `draining`, the job finished (SUCCESS) |
| `docker kill -s KILL` of the worker mid-job | the engine does **not** restart it (a manual stop; stated in the runbook); started by hand, healthy after 63 s; the job was taken back once its lease lapsed and the document arrived, 64 s after the kill (the default lease is 60 s) |
| Crash of the worker process **inside** its container (what an out-of-memory kill looks like) | restarted by the policy (restart count 0 to 1), healthy after 6 s; the job recovered in 62 s |
| Crash of the Python service mid-PDF | restarted by the policy in 7 s; the worker's call failed as a transient error and the retry produced the PDF, 8 s after the kill |
| Valkey restart (`docker compose restart valkey`) with an old record and a job in flight | old record SUCCESS, in-flight job SUCCESS, a new job SUCCESS (append-only file on); 8 s |
| API crash, then `docker compose stop api` | restarted by the policy in 6 s, a new job SUCCESS; SIGTERM exit 0 in 0 s |
| 24 different documents at once | 24 of 24, 0 failed, wall 2.3 s, p50 1.0 s, p95 2.0 s |
| Rollback: the application containers run another image tag on the same volumes | earlier document still served (HTTP 200), a new document SUCCESS, files 63 to 65 on the volume; switched back |
| Soak, 10 minutes, 1 request per second, mixed docx / HTML / PDF, three in four repeats | 600 requests, **0 failed**, p95 1.51 s, max 2.51 s, **0 restarts of any container**; memory in the "Memory" section |

**The first execution of this job found a real bug** (run 3, https://github.com/linearcombination-ai/DOC/actions/runs/45741220207): a job in flight when the worker crashed, or was killed, was never recovered and stayed in `Locating assets` (the kill and crash scenarios timed out after 240 s; in the soak six requests failed after 600 s in `Locating assets`: INFERRED to be requests whose key equalled a stuck job's, which share its task id, not checked in the logs). The worker id was `w<pid>`; in a container the process runs under an init process and has pid 7 on every start, so the restarted worker had the id of its dead predecessor, and the reaper (which, correctly, never takes back a delivery held by the calling worker's own id while the job is running) skipped the dead worker's job for good. The id is now unique per start (`w<pid>-<token>`, unit test); run 5 repeats every scenario green. None of the P6 host tests could see it (a restart on the host gets another pid).

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

## Docker and CI

All RAN on GitHub Actions on the fork `linearcombination-ai/DOC`, workflow `platform-rs`, branch `exp/platform-rs` (images are built into the runner and passed between jobs as tarballs; no registry, no secrets). Tool versions read from the runs: Docker engine 28.0.4, Compose v2.38.2, Rust 1.100.0, Python 3.13.18 (image) , Prince 16.1, WeasyPrint 68.1, Valkey 9.2.0, hadolint 2.15.1, actionlint 1.7.12, mypy 2.4.1, Playwright image v1.41.1 (DOC's own `testsDockerfile`), kitchen v0.1.24 `0090c0be`.

**Image sizes** (`docker image ls`, run 2) against DOC's own images built from DOC's Dockerfiles in the same run:

| This build | Size | DOC's stack | Size |
|---|---:|---|---:|
| `doc-rs/core` (api and worker, one image, two roles) | 187 MB | `wycliffeassociates/doc` (FastAPI and the Celery worker, one image; Calibre step removed) | 1,574 MB |
| `doc-rs/pyworker` (Python 3.13, Prince, WeasyPrint, fonts) | 746 MB | `doc-usfmparser` (.NET sidecar) | 294 MB |
| `doc-rs/gateway` (nginx) | 54 MB | `doc-fileserver` (nginx) | 142 MB |
| `doc-rs/frontend` (DOC's, unchanged) | 968 MB | frontend (the same image) | 968 MB |
| **Total for the stack** | **1,955 MB** (plus Valkey alpine) | **Total** | **2,978 MB** (plus Redis, Flower from the same API image) |

**Build times** (job wall seconds, `build-info` artifacts): cold cache (run 1) core **279 s**, frontend 123 s, DOC api 186 s, DOC usfmparser 52 s, gateway 5 s, DOC fileserver 5 s; pyworker cold: its apt, Prince and font layers took under a minute and the rest 90 s (run 1's build failed at the pip step, run 2 reused the earlier layers: 90 s; a fully cold pyworker build was not timed, INFERRED about 2.5 minutes). Warm (layer cache from the previous run, only the Rust sources changed in run 4): core 140 s (the cargo-chef dependency layer is reused, the workspace crates recompile), otherwise 18 to 110 s. The image build is not what costs time in this build; the Rust compile is.

**CI runs** (run URLs under https://github.com/linearcombination-ai/DOC/actions/runs/):

| Run | Commit | What it proved | Wall |
|---|---|---|---:|
| 45739179994 | `31a34eef` | Rust job green (clippy and the Rust tests against Valkey, 3 m 51 s); core image built; two red findings: hadolint 2.15 (JSON-form `HEALTHCHECK`) and `pip -c` refusing the extras in DOC's `requirements.txt` | 10 min 40 s |
| **45740243168** | `276ccb6f` | **M2: stack up and healthy in 26 s, smoke passed (Prince PDF, no fallback)**; **M3: DOC's `tests/e2e/doc` 334 / 0 / 132 on this stack and on DOC's FastAPI stack, 0 tests differ** | **7 min 54 s** |
| 45741220207 | `38b714d3` | DOC's Playwright tests: 17 / 1 / 10 against DOC's 18 / 0 / 10 (the book-name rule); ops run: **found the worker-id bug** (job lost after a container crash) | 31 min 53 s (ops) |
| 45744743681 | `62a6eb4b` | the proof job: with `DOC_BOOK_NAME_RULE=doc` the failing shard passes (7 of 7) | 9 min 47 s |
| **45744949895** | `6f17edd3` | **M5: worker-id fix; every ops scenario passes** (soak 600 requests, 0 failed) | 15 min |
| **45746933784** | `7ea614cd` | **the final run with every job** (`[all]`): static, Rust job (119 tests), 4 images, smoke, API tests on both stacks (334 / 0 / 132 = 334 / 0 / 132, 0 differ), Playwright (17 / 1 / 10 by default, the proof job 7 of 7), ops all pass; red only by the expected shard 1 | 15 min 30 s |
| 45748703874 | `d84ec596` | the same workflow with the expected shard-1 difference tolerated (`continue-on-error` on that one shard; the proof job stays strict): **the workflow run is green**, the tolerated job shows red inside it | 7 min 49 s |

Jobs of run 2 (seconds): Plan 3, Static checks 38, Rust tests 185 (cache warm), builds 23 to 127 (cache warm), smoke 102 (includes loading the images), e2e on this stack 295 / 315 / 321 per group, e2e on DOC's stack 330 / 340 / 274, pass table 15. The whole workflow without `ops` and the UI jobs is **7 min 54 s**; with the UI jobs about 10 minutes; `ops` (with its 10-minute soak) 14 min 30 s when every scenario passes (run 5; run 3 waited out its timeouts and took 30 minutes).

**What the first CI runs found that no host test could:** (1) the API role of the compose file had no `DOC_PDF_ENGINE` (every PDF request was a typed 501), found by the first local e2e run before any container existed; (2) hadolint 2.15's `DL3025` (the host had 2.12); (3) `pip` refuses `fonttools[woff]` in a constraints file; (4) **the worker id collision after a container restart** (below).

## DOC e2e pass table

DOC's own `tests/e2e/doc` (pytest; marker expression `not all and not randomized and not english_randomized`, 3 groups with `pytest-split` and `-n auto` as in DOC's CI), STET and Passages (`tests/e2e/stet`, `tests/e2e/passages`) **excluded and not run** (out of this phase). 466 test cases collected. Run 2, https://github.com/linearcombination-ai/DOC/actions/runs/45740243168. RS = this build, DOC = DOC's FastAPI + Celery + .NET sidecar stack, same run, same live catalog and content hosts. The skips are DOC's own markers and are identical.

| test file | tests | RS pass | RS fail | RS skip | DOC pass | DOC fail | DOC skip |
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

**Tests whose outcome differs between the stacks: 0.** Pytest seconds per group (1/2/3): this build 171 / 171 / 193, DOC's stack 174 / 200 / 183 (pytest's own time; the job totals above include starting the stack). The file list names `test_anchor_linking.py` (no test functions) and `test_english_combinatoric.py`, `test_randomized_combinatoric.py` (excluded by DOC's marker expression), which collect to nothing in this selection.

**DOC's Playwright tests** (`frontend/tests/e2e/doc_test_*.ts`; `passages_test*.ts` and `stet_test*.ts` excluded, listed as not run), 4 shards, against each stack:

| test file | tests | RS pass | RS fail | RS skip | DOC pass | DOC fail | DOC skip |
|---|---:|---:|---:|---:|---:|---:|---:|
| doc_test_1.ts | 5 | 4 | **1** | 0 | 5 | 0 | 0 |
| doc_test_2.ts | 5 | 2 | 0 | 3 | 2 | 0 | 3 |
| doc_test_3.ts | 4 | 3 | 0 | 1 | 3 | 0 | 1 |
| doc_test_4.ts | 3 | 3 | 0 | 0 | 3 | 0 | 0 |
| doc_test_5.ts | 7 | 1 | 0 | 6 | 1 | 0 | 6 |
| doc_test_6.ts | 2 | 2 | 0 | 0 | 2 | 0 | 0 |
| doc_test_7.ts | 2 | 2 | 0 | 0 | 2 | 0 | 0 |
| **total** | **28** | **17** | **1** | **10** | **18** | **0** | **10** |

The one difference (`doc_test_1.ts`, "books retained in basket on back button to languages and then forward") is **classified: a captain decision, not a defect.** The test selects Amharic and waits for the book name `2 ኛ ቆሮንቶስ` in the list. DOC normalises every name it reads (`normalize_localized_book_name`), which inserts the space between the digit and the ordinal sign; the standing rule says names are kept as written (`2ኛ ቆሮንቶስ`) and findings say what DOC would have shown. **Proof that this is the only cause:** `DOC_BOOK_NAME_RULE=doc` (a setting of the stack, default `written`) makes that shard pass 7 of 7 in CI (run 4, https://github.com/linearcombination-ai/DOC/actions/runs/45744743681, job "with DOC's book-name rule"), so with the rule on the table is 18 / 0 / 10 as DOC's. Playwright seconds per shard (run 3): this build 282 (the two 2-minute timeouts of the failing test) / 20 / 34 / 14, DOC's stack 137 / 69 / 117 / 50.

## Memory at rest and under load per container

RAN in CI (`docker stats`; at rest after the smoke test in run 2; under load sampled every 5 s through each e2e group, three groups pooled, run 2). A container's memory is what Docker reports (cgroup, includes page cache of its files).

| Container | At rest, after the smoke test (limit) | Under DOC's e2e load: median / peak |
|---|---|---|
| `api` (Rust) | 206 MiB (768 MiB) | 5 / 116 MiB |
| `worker` (Rust runner, git, core child) | 96 MiB (1 GiB) | 305 / 444 MiB |
| `pyworker` (Python docx, Prince, WeasyPrint) | 14 MiB (3 GiB) | 83 / 861 MiB |
| `valkey` | 5 MiB (256 MiB) | 5 / 5 MiB |
| `gateway` (nginx) | 5 MiB (128 MiB) | 6 / 6 MiB |
| `frontend` (DOC's) | 48 MiB (384 MiB) | not sampled in the API jobs |
| **Rust stack, sum of the peaks** | | **about 1.43 GB** (the peaks do not coincide) |
| DOC `worker` (Celery) | | 786 / 1,805 MiB |
| DOC `api` (FastAPI) | | 243 / 244 MiB |
| DOC `usfmparserapi` | | 50 / 65 MiB |
| DOC `redis`, `celery-dashboard` (Flower), `fileserver` | | 7 / 8, 40 / 41, 4 / 5 MiB |
| **DOC stack, sum of the peaks** | | **about 2.17 GB** |

**In the 10-minute soak** (run 5, one request per second, sampled every 10 s, 48 samples per container): `worker` median 116 / peak 118 MiB, `pyworker` 16 / 92, `api` 5 / 5, `frontend` 54 / 59, `valkey` 5 / 6, `gateway` 6 / 6 MiB; CPU peaks pyworker 124 %, worker 24 %. At rest before the ops scenarios: `api` 4, `worker` 4, `pyworker` 14, `valkey` 4, `gateway` 5, `frontend` 56 MiB (a freshly started stack; the catalog is not loaded until a lookup is made).

Notes: the API's 206 MiB after the smoke test is the catalog snapshot (about 20,000 rows, held once a lookup endpoint has been used); under the e2e load the API only enqueues, so it stays small. The pyworker's peak (861 MiB) is Prince and WeasyPrint on whole books; its 3 GiB limit was never approached. **CPU peaks** (percent of one core): pyworker 203, worker 112, api 3; DOC's Celery worker 394. After the smoke test the whole Rust stack holds about 375 MiB; a freshly started stack about 87 MiB (the soak paragraph below).

## Weaknesses

* **One of DOC's 28 UI tests differs by default** (the book-name rule, above): a captain decision with a one-line switch and a CI proof, but the default table reads 17 / 1 / 10 until the captain says whether DOC's normalisation or the translators' text should be the default.
* **The docx is still the hand-written generator** (the proposed look's Word styles), not DOC's python-docx output; DOC's docx tests are text-based and pass, but nothing was opened in Word (the captain's check). HTML and PDF are DOC's own markup and CSS.
* More hand-written code to own than the other build's queue library would be: about 1,700 lines of runner, store, lock and Lua, plus the SMTP client; 119 Rust tests (117 in run 2, plus the book-name-rule and worker-id tests), all written by the same author.
* **A deterministic environment found a bug the host could not:** the worker id was the process id, and in a container the process id repeats on every restart (the reaper then never took a dead worker's job back). Fixed and unit-tested, but it shows that the P6 fault tests (which restart with a different pid) did not cover "same identity after restart"; the ops run found it on its first execution.
* Email is at most once, not exactly once (stated above). The core child costs about 10 ms per job and forces `Assembling content` to be published after it returns.
* Prince runs in its non-commercial mode (a notice on page one), as DOC's own image does; the licence is not decided. Where the CPU architecture has no Prince package the build succeeds and PDFs come from WeasyPrint with a finding (CI built x86_64 only).
* Redis or Valkey without persistence loses queued state (documented, tested in P6); the stack runs Valkey with the append-only file, and the Valkey restart scenario passed in CI; Sentinel and Cluster were not tried.
* Disk: the output and assets volumes grow until the janitor and the 30-minute window expire them; no volume size limit is set in compose.
* `DOC_TEMPLATES_DIR` ties the service to DOC's template files (copied into the image from `backend/templates/html`); a changed template changes the output.

## What was not done

* Opening any output in Word, or visually comparing HTML/PDF with DOC's (the captain's check); the OOXML validator on the P7 docx.
* STET and Passages: routes and tests (`tests/e2e/stet`, `tests/e2e/passages`, `frontend/tests/e2e/stet_test*.ts`, `passages_test*.ts`) are out of this phase and were not run.
* DOC's randomized and `all` e2e selections (excluded by DOC's own marker expression), DOC's `tests/unit`.
* `docker compose up` on the captain's own machine, arm64 images, Docker Desktop on macOS or Windows (CI built and ran x86_64 on Linux runners only); a fresh-clone rehearsal of the README on a second machine (CI starts from a clean checkout, which covers the compose path, not the README text).
* A container hitting its memory limit (the engine's OOM kill), CPU throttling, soaks longer than 10 minutes in containers (the P6 host soak was 31 minutes), faults injected during a soak, more than 24 documents at once in containers, multi-host, image vulnerability scanning, Kubernetes.
* The Redis 8 suite in containers (CI ran Valkey 9.2.0; P6 passed the suite on both on the host).
* The TypeScript build's own pass numbers were read (its report and checkpoint), not re-run; its code was not copied except where the report names it (CI scripts, the USFM repair module).
