# Running the stack as plain processes (no docker): the developer path

Experiment only. Nothing in DOC is changed, nothing is pushed, no pull request. This is the second half of a two-build comparison (the other build is a Rust shell in `data/doc-p6-rs`); the shell choice stays open until you have compared both in code and in use.

* Report: `../report.md`. Plan: `docs/P6-PLAN.md`. Operations: `docs/RUNBOOK.md`. Evidence tables: `docs/COMPARISON.md` (same headings in both builds).
* What it is: a service that runs **beside DOC's FastAPI** (same paths, same status strings, same cache key) and replaces nothing: Hono API (`src/main.ts`), BullMQ workers (`src/worker-main.ts`), the Rust core as wasm (`core/`), and a supervised Python docx/PDF seam (`python/`).

## Ten-minute try-it path (no docker)

Layout requirement: this directory sits one level under the DOC worktree root and the kitchen clone is at `<worktree>/_exp/kitchen` (`core/crates/doc-canon/Cargo.toml` paths); copy it elsewhere and those paths must be changed. Tools needed, all on a laptop: Node 26 (24 works), a Redis 8 or Valkey 9 server binary (built from source here, `BUILD_TLS=yes`; a distro package is fine), Python 3.12+, git, `curl`. To rebuild the wasm core: Rust stable with the `wasm32-unknown-unknown` target and the `wasm-bindgen` CLI 0.2.130. Where each was obtained on this host is in `docs/COMPARISON.md` (Run it).

```bash
cd prototype                                   # this directory
bash scripts/build-wasm.sh                     # 2.5 min cold: core/pkg (full, 2.97 MB) and core/pkg-api (light, 0.78 MB)
npm ci                                         # 11 s
export REDIS_SERVER_BIN=/path/to/redis-server  # or valkey-server
bash scripts/stack.sh start                    # Redis + catalog mock + local git remotes + API :18301 + 1 worker; ~3 s
```

`stack.sh start` serves Jude (ULB, TN, TQ, TW) from local git remotes through a mock of DOC's catalog, so nothing needs the network. `REAL_NETWORK=1 bash scripts/stack.sh start` uses the real catalog and content host instead (then ask for `en` books as DOC does).

```bash
API=http://127.0.0.1:18301
REQ='{"email_address":null,"assembly_strategy_kind":"lbo","layout_for_print":false,"generate_pdf":false,"generate_epub":false,"generate_docx":true,"limit_words":true,"document_request_source":"ui","resource_requests":[{"lang_code":"en","resource_type":"ulb","book_code":"jud"},{"lang_code":"en","resource_type":"tn","book_code":"jud"}]}'

# 1. a document job and its status strings (DOC's own strings, matched literally by the frontend)
curl -s -D- -X POST $API/documents_docx -H 'content-type: application/json' -H 'x-request-id: try-1' -d "$REQ"   # {"task_id":"1"} ; x-request-id echoed
for i in 1 2 3 4 5 6 7 8; do curl -s $API/task_status/1; echo; sleep 0.1; done      # Receiving request ... Converting to Docx ... {"state":"SUCCESS","result":"en_ulb-tn_jud_7af47830c746"}
ls run/output                                                                       # the .docx and its .findings.json

# 2. duplicate requests coalesce: five at once, one job, one document (x-doc-deduplicated: 1 on the four joiners)
REQ2="${REQ/\"layout_for_print\":false/\"layout_for_print\":true}"                    # another document key
for i in 1 2 3 4 5; do curl -s -D- -o /dev/null -X POST $API/documents_docx -H 'content-type: application/json' -d "$REQ2" | grep -i -E "x-doc-dedup|HTTP/"; done

# 3. a killed worker recovers: two workers, a slow docx step (4 s), SIGKILL one mid-job, the job still finishes, once
bash scripts/stack.sh stop
WORKERS=2 DOCX_FAULT=slow:4000 LOCK_DURATION_MS=3000 STALLED_INTERVAL_MS=1000 bash scripts/stack.sh start
REQ3="${REQ/\"limit_words\":true/\"limit_words\":true,\"use_chapter_labels\":true}"     # a new document key
ID=$(curl -s -X POST $API/documents_docx -H 'content-type: application/json' -d "$REQ3" | sed 's/[^0-9]//g')
sleep 1.5; curl -s $API/task_status/$ID; echo                                       # Converting to Docx
for n in 1 2; do grep -q "\"job\":\"$ID\"" run/worker$n.log && W=$n; done; kill -9 $(cat run/worker$W.pid)   # kill the worker that holds it
for i in $(seq 1 20); do curl -s $API/task_status/$ID; echo; sleep 1; done   # ... SUCCESS from the other worker after the lock expires (about 7 to 12 s)
grep -h -E "stalled" run/worker*.log | cut -c1-200

# 4. findings: the routes and the header
curl -s -D- $API/task_status/1 | grep -i x-doc-findings                              # count, on a finished job
curl -s $API/findings/documents/en_ulb-tn_jud_7af47830c746                            # what the sources had wrong, as data ([] when none)
curl -s "$API/book_codes_for_lang/en?findings=1" | head -c 300                        # the lookup endpoints can wrap their answer with the findings

# 5. the typed 503 when the catalog is down and nothing is cached (fixed mode)
bash scripts/stack.sh stop; printf 'DATA_API_URL=http://127.0.0.1:9/\n' > run/down.env     # a catalog URL that refuses connections
DATA_STACK=external STACK_ENV=$PWD/run/down.env bash scripts/stack.sh start
curl -s -D- $API/language_codes_and_names                                            # 503 {"kind":"catalog_unavailable","transient":true,...} with Retry-After
curl -s -X POST $API/documents_docx -H 'content-type: application/json' -d "$REQ" ; echo  # accepted (queued); the job retries with backoff until the catalog returns

# 6. other behaviours
curl -s $API/metrics | head -30                                                     # Prometheus text
curl -s $API/health/status                                                          # Redis, queue lag, workers, docx pool, last catalog fetch, disk
curl -s -X POST $API/documents_docx -H 'content-type: application/json' -d '{"resource_requests":"no"}'   # 422 in DOC's legacy body
bash scripts/stack.sh stop
```

(The numbered blocks are what `docs/COMPARISON.md` "Run it" shows running, with output and timings; the clean-copy rehearsal is recorded there too.)

## Layout

```
core/            Rust workspace (P3b crates + doc-job): job_key, job_parse, job_assemble; built twice: full (pkg) and light (pkg-api)
src/             main.ts (API) · worker-main.ts · app.ts (routes) · config.ts · jobs/ (queue, worker, pipeline, docx, docxpool, repolock, email, redis, config)
                 acquirer/ (git clones, atomic generations, Redis clone lock) · catalog/ · service/lookup.ts · obs/ (metrics, telemetry) · contract/ (zod schemas = the contract)
python/          docx_core.py (shared) · docx_worker.py (one job per process) · docx_server.py (persistent, JSON lines) · docxgen/ (the template-free generator)
generated/       openapi.json and api.d.ts (TypeScript types for the SvelteKit frontend, from the contract)
test/            real processes, no fakes: Redis/Valkey, git, SMTP server (smtp-server), Python, wasm
tools/           local-stack.ts · queue-admin.ts · soak.ts · docx-bench.ts · api-memory.ts · doc-celery/ (DOC's own stack) · e2e/ (DOC's e2e tests pointed here)
scripts/         build-wasm.sh · stack.sh · run-suite.sh · mypy.sh · gen-contract.ts
docs/            P6-PLAN.md · RUNBOOK.md · COMPARISON.md
results/         every number in the report, as the tools printed it
```

## Tests and checks

```bash
REDIS_SERVER_BIN=/path/to/redis-server npx vitest run     # whole suite, files one at a time
bash scripts/run-suite.sh /path/to/valkey-server valkey9  # the same suite against Valkey, result kept in results/
bash scripts/mypy.sh                                      # mypy --strict over every Python file (set MYPY=/path/to/mypy)
npx tsc --noEmit                                          # strict TypeScript, no `any` (grep in COMPARISON)
node scripts/gen-contract.ts                              # regenerate generated/openapi.json and api.d.ts (needs tools/gen: npm ci there)
```
