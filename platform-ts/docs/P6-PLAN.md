# P6 plan, TypeScript shell (task doc-p6-ts)

Written before building. Experiment only: no push, no PR, nothing in DOC changed. Starting point: the P5 prototype
(Hono + BullMQ 6.7.10 + ioredis 6.0.0, Rust core as wasm, Python docx seam). DOC base commit `50de21cd`.
Evidence tags used throughout: RAN, READ, INFERRED.

## Scope restated for this build

| id | item | done when |
|---|---|---|
| A1 | Per-repository single flight across worker processes: Redis lock (`SET NX PX`) with a lease, renewal and a fencing token | a test starts two worker processes with an empty cache and counts `git clone` calls: one per repository |
| A2 | Persistent supervised Python docx worker (JSON lines on stdin/stdout), restart on crash, per-job deadline kills and replaces it, memory ceiling, recycle after N jobs; per-job-process mode stays; a switch | tests for crash, hang, memory ceiling, recycle; measured cost per job for both modes |
| A3 | Cancellable CPU stages: a hung core call is killed and the job retried | a test with a hang inside the core thread (JS spin and, if feasible, a wasm loop) shows kill, retry, success |
| A4 | Catalog normalisation off any thread that must stay responsive | event-loop gap test while normalising the 21,000-row snapshot |
| B | Same fault suite against Valkey and Redis 8; both built from source; licence read from the source tree; persistence settings documented | suite parametrised by server binary; table of results |
| C | Structured log line per stage and attempt with request id from HTTP to worker to Python; Prometheus `/metrics`; extended `/health/status`; RUNBOOK | tests read the log lines; metrics scraped and checked; request id appears in the Python log |
| D | Config validation at start; Redis auth and TLS settings; request size limit; path validation defaults (language 1..35 `[A-Za-z0-9-]`, at most 100 book codes in fixed mode); graceful shutdown with drain deadline; disk-space guard; scratch cleanup of dead attempts at start; output cache expiry (30 min default, configurable) | unit and process tests for each |
| E | HTML output for `POST /documents`; the other request shapes the core supports (TW, TQ, TNC, BC, per-book resources); email step with its own idempotency key and a fake SMTP server | a test sends the job twice and through a forced redelivery and counts exactly one message; PDF is a stretch: typed 501 if not reached |
| F | Soak of at least 30 minutes of mixed requests, memory sampled; 20 or more concurrent jobs; real-network cold run if the network allows; DOC's own API e2e tests pointed at this service, pass-rate table against FastAPI | result files and tables |
| G | Generated TypeScript types for the SvelteKit frontend from the contract; type-check against how `frontend/src` calls these endpoints, report mismatches | `tsc` over a check file built from frontend call sites |
| H | Python: no `typing.Any`, `mypy --strict` clean | mypy run over `python/` and over DOC's own gate paths unaffected |
| X | Light API process: it loads only the part of the wasm core it needs; report the memory effect | RSS of API process before and after |

## Ordered task list

1. Environment: node 26, Rust (present in `_exp`), wasm-bindgen CLI 0.2.130, Redis 8 and Valkey built from source, local mirrors and the mock data API. Test: P5's 92 tests still pass unchanged.
2. Plan, checkpoint, commit.
3. D (config validation first, because every later item reads config).
4. C (logging with request id, metrics, health), since the later tests assert on log lines and counters.
5. A1 (clone single flight), A3 and A4 (core thread cancellable; normalisation on the core thread).
6. A2 (persistent Python worker) with the cost measurement.
7. E (HTML, other request shapes, email with idempotency and fake SMTP; PDF 501 or stretch).
8. B (Valkey and Redis: whole fault suite parametrised, licence reading).
9. X (API light mode) and H (mypy strict).
10. G (frontend types).
11. F (scale: concurrency 20+, DOC e2e pass-rate table, real network run, then the soak last, alone, host load below 3).
12. RUNBOOK, COMPARISON, README, report; clean-copy rehearsal of every README command.

## Tests that prove each item

Listed in the table above; every test runs real processes against a real server (no in-process Redis fake). Each fault test is
recorded in the COMPARISON fault table with its result.

## Risks

* Host: 4 cores, 11 GB shared with the Rust build's worker; load was 3.1 at start. Heavy steps (cargo, Redis and Valkey builds, soak) run one at a time and the load is recorded.
* The core's job ops are built from the P3b crates; a wasm infinite loop may not be reproducible without editing the core (a test hook): then the claim stays at "JS spin on the same thread".
* DOC's e2e tests expect the FastAPI contract in places this build does not implement (for example endpoints beyond the first slice); differences are classified, not hidden.
* The real catalog and content host may be unreachable or rate limited; then the real-network run is reported as not done.
* TypeScript 7 and Node 26 type stripping: strictness without `any` limits how fast the zod and Hono typings go.
* Scope is very large; items are ordered so that the hardening (A to D) is complete before the extras (E to G). What is not reached is named in COMPARISON "What was not done".
