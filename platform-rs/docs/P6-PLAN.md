# P6 plan, Rust shell (task `doc-p6-rs`)

Written before building. Start point: the P5 Rust prototype (`data/doc-p5-spike-rs/prototype`, DOC commit
`50de21cd3bab5c6f48961da64257118f6f0ead0e`). The other build (`doc-p6-ts`) is compared against this one with the
same headings in `docs/COMPARISON.md`. Evidence tags: **RAN** (executed here), **READ**, **INFERRED**.

Standing rules this plan keeps: the core is Rust; docx stays Python behind a seam; a request must end up with a
document (retry, self-heal, generous ceilings, clear logs, never a bare error for a transient condition); names are
kept as written and findings are returned; the service runs beside DOC's FastAPI and replaces nothing; the captain
does the final visual check in Word. No `unsafe`, no `unwrap`/`expect`/`panic` on input paths in Rust, no `typing.Any`
and `mypy --strict` clean in Python. No docker (it cannot be run here); the gateway story is documented, not built.

Host limits: 4 CPUs, 12 GB shared with the other worker; `CARGO_BUILD_JOBS=2`; one heavy job at a time; benchmarks and
the soak only with load below 3 (the load is recorded next to each number); every server stopped when idle.

## Ordered task list and the test that proves each item

| # | Item (brief letter) | What is built | The test that proves it |
|---|---|---|---|
| 0 | Environment | toolchain on the worktree volume, Redis 8.12.0 and Valkey 9.2.0 built from source, kitchen v0.1.24 read-only, baseline `cargo test` of P5 green | `results/baseline-tests.log` (26 P5 tests) |
| 1 | Config (D) | one `Config` loaded from the environment with every problem collected and reported at start; sensible limits; `DOC_*` names, DOC's own names kept | unit tests: every bad value yields a named error, several at once; the binary exits 2 with the list |
| 2 | Single flight across processes (A) | `RepoLock`: Redis `SET NX PX` lock per repository URL with a lease, renewal and a fencing token (`INCR`), compare-and-delete release; wraps the acquirer so the second worker finds a fresh cache | test with **two worker processes** and a counting fake `git`: N jobs needing one repository -> exactly **one clone**; a SIGKILLed lock holder: the lease expires and the other worker takes over (clone count 2, document still produced); a stale holder's late release does not remove the new holder's lock |
| 3 | Core off the runtime, cancellable (A) | CPU stages (parse + assemble) run in a child process of the same binary (`doc-jobd core-stage`) that can be killed; deadline kills the process group; the job is retried | test: a hook makes the child hang on attempt 1 -> killed at the stage deadline (pid gone), retry completes; memory of the parent stays flat |
| 4 | Catalog normalisation off responsive threads (A) | the catalog parse and index build moves to the blocking pool in `doc-app`/`doc-catalog` | test: while a large synthetic catalog is normalised, `/health/status` p99 stays under 50 ms |
| 5 | Persistent docx worker pool (A) | `python/docx_worker.py --serve` (JSON lines on stdin/stdout), supervised by a Rust pool: restart on crash, per-job deadline kills and replaces the worker, RSS ceiling and recycle after N jobs, switch `DOC_DOCX_MODE=per_job|persistent` | tests: crash mid-job -> worker replaced and job retried; hang -> killed at deadline and replaced; recycle after N; RSS ceiling recycle; measured cost per job in both modes (`results/seam-cost.json`) |
| 6 | Observability (C) | request id (`X-Request-Id`, generated when absent) stored with the job, put on every log line of the worker, passed to the Python process and logged there; one structured line per stage and per attempt; `GET /metrics` in the Prometheus text format; extended `/health/status` | tests: the same id appears in the API log, the worker log and the Python line; `/metrics` text parses (checked by a strict parser in the test) and each required family is present and moves; `/health/status` shows Redis, queue lag, last catalog fetch, docx pool |
| 7 | Operational safety (D) | Redis AUTH and TLS (`rediss://`, CA file) tested against a TLS Redis; request body limit; path and language-code validation defaults (1 to 35 letters, digits, hyphens; at most 100 book codes in fixed mode) on the job request too; graceful shutdown with a drain deadline; disk-space guard; scratch cleanup of dead attempts at start; output expiry janitor (DOC's 30 minutes, configurable) | tests: AUTH refused without password and accepted with; TLS connect with the right CA and failure with the wrong one; 413 on an oversized body; 422 on bad codes; disk guard answers a typed 503 + retry; janitor removes only expired outputs; start-up cleanup removes dead scratch |
| 8 | HTML for `POST /documents` (E) | job kind `html`: core `as_built` parts -> DOC's compose, issue check, title page and header templates (rendered in Rust) -> `<key>.html`; same status strings plus `Converting to PDF` only when PDF is asked | test: HTML equals the structure DOC's oracle gives for the P5 request (parts equal, wrapper checked), valid and served as the key |
| 9 | Other request shapes (E) | Translation Words (link and include), Translation Questions, Bible Commentary, TN commentary, by-book and by-chapter strategies, two-language and two-column layouts as far as the core supports them; unsupported shapes answer a typed 501 or a finding, never a bare 500 | table test: every shape the UI can post (read from `frontend/src`) is run end to end with fixtures; result table in COMPARISON |
| 10 | Email (E) | step after the document: `lettre` SMTP client, STARTTLS/TLS/plain by config, an idempotency record in the store (`none -> sending -> sent`) keyed by request key and recipient, claimed before the send and fenced | tests with a fake SMTP server: normal send = 1 message; job retried after the send = still 1 message; redelivery (SIGKILL after the send, before completion) = still 1; SMTP refusing twice then accepting = 1; crash between claim and send = at most 1 and a finding |
| 11 | PDF (stretch) | `POST /documents` with `generate_pdf` answers a typed 501 unless the Python seam with WeasyPrint is reached; engine switch kept | typed 501 test; if reached: a PDF produced through the seam |
| 12 | Valkey (B) | the whole fault suite runs against `DOC_SERVER_BIN=valkey-server` as well as Redis 8; licences read from each source tree; persistence settings documented | `results/faults-redis.log`, `results/faults-valkey.log`, side-by-side table |
| 13 | Evidence at scale (F) | soak >= 30 minutes of mixed requests with RSS sampled; 20+ concurrent jobs; a real-network cold run of the P5 request; DOC's `tests/` run against this service where they can be, pass-rate table against FastAPI on the same tests | `results/soak.csv`, `results/concurrency.json`, `results/real-network.json`, `results/e2e-table.md` |
| 14 | Frontend types (G) | the contract now includes the document endpoints; `contract/api.d.ts` generated; a drop-in check against how `frontend/src` calls the endpoints, without changing the frontend | `tsc --strict` over the drop-in file; mismatches listed |
| 15 | Python side (H) | `docx_worker.py` and every script typed, no `Any`, `mypy --strict` clean; also run over this directory the way DOC's gate would | mypy run output |
| 16 | Release build time | measure and minimise (release profile, dependency features, linker) | `results/release-build.log` before and after |
| 17 | Apalis re-check | latest apalis-redis, the same five defects as P5 | `spikes/apalis` rerun, result recorded |
| 18 | Docs and clean-copy run | README ten-minute path, RUNBOOK, COMPARISON; every README command exercised from a clean copy | `results/readme-run.log` |

## Risks

1. **Size.** The brief is large (items A to H). Order above is by value to the captain's comparison; items 11 (PDF) and
   the e2e pass-rate table (13) are the most likely to be partial, and will be reported as such.
2. **Host.** Redis/Valkey builds, release builds and the soak all want CPU; they run one at a time and only when the
   load is under 3 (soak and benchmarks) or after telling firstmate (anything over 3 GB).
3. **Hand-written runner.** More Lua is more risk; every new Lua script gets a contract test on both stores.
4. **Child-process core.** Passing repository content to a child costs time; the child reads repository directories
   itself (they are on disk), only the request and the result cross the pipe.
5. **Email semantics.** "Never twice" and "always once" cannot both hold across a crash between the claim and the send;
   the plan picks at-most-once for the email and records the unknown case as a finding (decision for COMPARISON).
6. **E2E against DOC's tests.** They need the real catalog and clone hosts and a running DOC stack for the baseline;
   if the network or the .NET sidecar blocks a baseline, the table says which rows could not be compared.
7. **Release build time** was 11 minutes in P5.
8. **Other build's findings** are read at each milestone from `data/doc-p6-ts/checkpoint.md` and adopted where they
   apply; the report names what was taken.

## Outcome per item (added at the end; evidence in COMPARISON.md and the report)

0 to 10, 12, 14 to 18: done and proved by the test named in the table (items 2 to 7 and 10 by `crates/doc-jobd/tests/p6.rs`, 4 by `crates/doc-shell/tests/responsive.rs`).
8 HTML: done. 9 shapes: done for 17 shapes as docx and HTML (a table, not a gap list). 11 PDF: done through WeasyPrint (Prince not exercised).
13 evidence at scale: soak 31 min clean, 24-job burst, real-network cold run done; DOC's own e2e: `test_api.py` only, 18/31 against the baseline's 23/31, three parity gaps classified and not fixed.
Changes to the plan while building: the core child runs parse and assemble together (one process, not two); the release-build "fast profile" gave no gain and was dropped; the clone-lock opt-out `DOC_JOBD_CLONE_LOCK=false` was added as a test control.
