# DOC platform experiment, P6: the Rust shell, hardened

An axum service (`doc-jobd`) that runs **beside** DOC's FastAPI and replaces nothing: the seven read endpoints
(with the findings routes), `POST /documents_docx`, `POST /documents` (HTML), `GET /task_status/{id}`,
`GET /health/status`, `GET /metrics`. The core is Rust (scripture-kitchen v0.1.24 behind a thin adapter), the job runner is
hand-written on Redis Streams and Lua (Valkey works too), and docx generation stays Python behind a seam. Nothing here changes
DOC; it is an experiment kept apart so it can be compared with the TypeScript build (`docs/COMPARISON.md`).

Docs: `docs/P6-PLAN.md` (what was planned and how each item was proved), `docs/RUNBOOK.md` (operating it),
`docs/COMPARISON.md` (the evidence, same headings as the other build).

## Ten-minute try-it path

You need Linux, `git`, `python3` (3.10+), a C toolchain (for Redis/Valkey), and the Rust toolchain used here (rustc 1.100).
No docker. Everything lives under `p6rs/`; the one thing outside is a read-only clone of kitchen.

```bash
cd p6rs
. ./env.sh                       # toolchain (CARGO_HOME, RUSTUP_HOME, CARGO_TARGET_DIR on this volume), CARGO_BUILD_JOBS=2
git clone --depth 1 --branch v0.1.24 https://github.com/WycliffeAssociates/scripture-kitchen _exp/kitchen   # once; read-only dependency
scripts/build-servers.sh         # once: Redis 8.12.0 and Valkey 9.2.0 from source into .p6env/ (about 10 and 6 minutes; skip Valkey if you like)
cargo build --release -p doc-jobd          # the service; binary target/release/doc-jobd (about 6 minutes clean; no faster profile helped, see COMPARISON)
```

The first build is the slow part. If you already have a Redis or Valkey, skip `build-servers.sh` and point `REDIS_BIN` at it.

Start the stack (a data API mirror, Redis, one `doc-jobd`; the repositories are the fixtures and `git` is a fake that copies
them, so nothing needs the network):

```bash
. scripts/stack.sh && stack_up redis       # or: stack_up valkey      (API on http://127.0.0.1:8081)
curl -s localhost:8081/health/status | python3 -m json.tool | head -20     # ready, redis, queue lag, processes, docx pool, catalog, disk
```

To use the real catalog and the real content host instead of the mirror and the fake git:
`export DATA_API_URL=https://api.bibleineverylanguage.org/v1/graphql REAL_GIT=1` before `stack_up`.

### 1. A document job and its status strings

```bash
post_docx                                   # POST /documents_docx with the English Jude request (ULB + TN); prints the response
id=$(post_docx | tail -1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["task_id"])')
watch_task "$id"                            # prints each new state once
```
Expected: `{"state":"PENDING"}`, then `Receiving request`, `Locating assets`, `Provisioning asset files`, `Parsing asset files`,
`Assembling content`, `Converting to Docx`, then `{"state":"SUCCESS","result":"en_ulb-tn_jud_<hash>"}`: DOC's exact strings,
the ones the UI matches. The file is `scratch/stack/out/<result>.docx` (and `<result>.findings.json`). `post_html` does the same for
`POST /documents` and `<result>.html`.

### 2. Duplicate requests coalescing

```bash
body=$(jude_json '{"layout_for_print": true}')
pids=(); for i in 1 2 3 4 5 6 7 8; do post_docx "$body" | grep -iE '^x-doc-coalesced|task_id' & pids+=($!); done; wait "${pids[@]}"
```
One task id is returned eight times, and seven responses carry `x-doc-coalesced: true`: one job, one run.

### 3. A killed worker recovering

```bash
daemon_stop a; sleep 1.5
daemon_start a DOCX_WORKER_FAULT=slow:8 DOCX_WORKER_FAULT_TIMES=1 DOCX_WORKER_FAULT_STATE=$P6/scratch/stack/fault.state
body=$(jude_json '{"use_chapter_labels": true}'); id=$(post_docx "$body" | tail -1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["task_id"])')
sleep 3; daemon_kill a                       # SIGKILL in the middle of the docx stage
daemon_start b                               # a second worker (port 8082)
PORT_API=8082 watch_task "$id"               # the job finishes on attempt 2, a few seconds after the lease expires
$P6/.p6env/redis/bin/redis-cli -p 6399 HGET docjobs:job:$id attempt
```
The job completes exactly once with a valid docx (`attempt` is 2); the orphaned Python process leaves by itself.

### 4. The findings routes and header

```bash
curl -si localhost:8081/book_codes_for_lang/en | head -12            # DOC's exact JSON, plus X-Doc-Findings: N
curl -s  localhost:8081/findings/book_codes_for_lang/en | python3 -m json.tool | head -30     # {data, findings, mode}
```

### 5. The typed 503 when the catalog is down

```bash
mirror_down                                  # the data API mirror starts answering 503 (SIGUSR1 toggles)
daemon_start c                               # a fresh process (port 8083) that has never had a catalog: nothing cached
curl -si localhost:8083/book_codes_for_lang/es-419 | head -12        # 503 {"code": ..., "retryable": true} with Retry-After
mirror_up; daemon_stop c
```
(A process that already holds a catalog keeps answering from it, with a `catalog_served_stale` finding after the cache window.)

### 6. More behaviours

```bash
curl -s localhost:8081/metrics | grep -E '^doc_(queue_depth|jobs_total|job_attempts_total|stage_duration_seconds_count|repo_clones_total|workers_alive)'
curl -si -X POST localhost:8081/documents -H 'content-type: application/json' -d "$(jude_json '{"generate_pdf": true, "generate_docx": false}')" | head -3   # 501 pdf_not_implemented (typed) unless DOC_PDF_ENGINE=weasyprint and DOC_PDF_PYTHON=<interpreter with weasyprint> are set when the daemon starts, then a PDF job runs
python3 -I scripts/fake_smtp.py --port 2525 & sleep 1                 # email demo: a fake SMTP server that prints what it receives
daemon_stop a; sleep 1.5; daemon_start a SEND_EMAIL=true SMTP_HOST=127.0.0.1 SMTP_PORT=2525 SMTP_TLS=none
post_docx "$(jude_json '{"email_address": "translator@example.org", "layout_for_print": false, "use_chapter_labels": true}')" | tail -1   # then watch the fake server print the message (once)
```
Redis down: `kill $(cat scratch/stack/redis.pid)`, then `post_docx` answers a typed 503 `queue_unavailable` with `Retry-After`; restart
Redis (the `stack_up` line for it) and the service resumes by itself.

Stop everything: `stack_down` (and `kill %1` for the fake SMTP server).

## Tests

```bash
export DOC_SERVER_BIN=$P6/.p6env/redis/bin/redis-server          # or .p6env/valkey/bin/valkey-server
cargo test --workspace -- --test-threads=2                        # about 4 minutes cold (debug build), about 90 s warm; 100+ tests
MYPYPATH=../backend ../_exp/venv/bin/mypy --strict python scripts   # the Python side: strict, no Any
```
`crates/doc-jobd/tests/faults.rs` (15 fault tests, from P5), `crates/doc-jobd/tests/p6.rs` (what P6 added), `crates/doc-shell/tests`
(the read endpoints). The TypeScript types: `contract/api.d.ts`, checked by `contract/frontend-dropin.ts` (see `docs/COMPARISON.md`).

## Layout

```
crates/doc-contract    wire types, validators, the OpenAPI document (read endpoints and job endpoints)
crates/doc-lookup/-app/-catalog/-acquire/-kitchen/-ports/-flight/-shell    the P4 shell (catalog, git acquirer, names, read endpoints)
crates/doc-request/-interleave/-md/-canon/-model    the P3b core (kitchen v0.1.24 read-only through ../_exp/kitchen)
crates/doc-jobs        runner, Redis/Valkey and memory stores, Lua, pipeline, core child, docx seam and pool, email, html, metrics, config
crates/doc-jobd        the binary: roles, signals, subcommands check-config, print-openapi, core-stage; the integration tests
python/                docx_worker.py (per-job and --serve) and the template-free docx generator
scripts/               stack.sh, build-servers.sh, loadgen.py (soak, burst, real network), mock_data_api.py, fake_smtp.py, replay.sh, ...
contract/              openapi.json, api.d.ts, frontend-dropin.ts (must compile), frontend-mismatch.ts (fails on purpose)
results/               every number in COMPARISON.md, as the tools wrote it
```
