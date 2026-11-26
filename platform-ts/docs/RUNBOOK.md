# RUNBOOK: the TypeScript build as a container deployment (P7)

For whoever is on call. Everything here was run on this prototype; commands are copy-paste. Evidence tags: RAN (executed in this task), READ (read in code or docs), INFERRED (reasoning, not executed).

## 0. The container deployment (P7)

One file, `platform-ts/docker-compose.yml`, starts the whole system. Evidence: every statement marked RAN was executed in GitHub Actions on the fork (run URLs in `COMPARISON.md`); nothing here was run with a container engine on the authoring host (it has none).

| Service | Image (built from) | Ports (host) | Volumes | Limits (default, override with the variable) | Health check |
|---|---|---|---|---|---|
| `valkey` | `valkey/valkey:9` (BSD-3) | none (internal 6379) | `valkey-data` (AOF `everysec`, `noeviction`; `docker/valkey.conf`) | 256 MB (`VALKEY_MEM_LIMIT`), 1 CPU | `valkey-cli ping` every 5 s |
| `volume-init` | the worker image, runs once | none | the three data volumes | 64 MB | exits 0 when the volumes are writable by uid 1000 (api and worker wait for it) |
| `api` | `docker/Dockerfile.shell` (node 26, light wasm module, no Python, no git) | none (internal 18301, reached through `gateway`) | `assets`, `output`, `work` | 768 MB (`API_MEM_LIMIT`), 1 CPU | `GET /health/status` |
| `worker` | `docker/Dockerfile.worker` (python 3.13 + node + full wasm module + Prince + fonts) | none (internal 18302 health) | `assets`, `output`, `work` | 3 GB (`WORKER_MEM_LIMIT`), 2 CPUs; `WORKER_CONCURRENCY` 2, `DOCX_POOL_SIZE` 2 | worker heartbeat `/health` |
| `gateway` | `docker/gateway` (nginx:alpine, DOC's file-server config plus the API reverse proxy) | **5005** (API), **8089** (files) | `output` read only | 128 MB | `/gateway-health` |
| `frontend` | DOC's own `frontend/Dockerfile`, unchanged | **8001** | none | 512 MB | `/health` |

Commands (run in `platform-ts/`):

| Task | Command |
|---|---|
| First start (builds the images) | `docker compose up --build` ; open http://localhost:8001 |
| Start detached and wait | `docker compose up -d --build && bash ci/wait-healthy.sh 300 valkey api worker gateway frontend` |
| What runs, what is unhealthy | `docker compose ps -a` ; `docker stats --no-stream` |
| Logs of one service (JSON lines, one per stage and attempt, with the request id) | `docker compose logs -f --since 10m worker` |
| One request end to end | `bash ci/smoke.sh` (health, lookups, docx, html, pdf, UI) |
| Graceful stop | `docker compose stop` (the worker gets 135 s to finish running jobs: `stop_grace_period`) ; `docker compose down` keeps the volumes, `down -v` removes documents, clones and queue |
| Restart one service | `docker compose restart api` (restart policy `unless-stopped` also brings a crashed service back) |
| Use Redis 8 instead of Valkey | `docker compose` with the `valkey` service's image replaced by `redis:8` (the suite passes on both; Redis 8's licence is RSALv2/SSPLv1/AGPLv3 at the operator's choice) |

**Volumes and ownership.** Both application images run as the same non-root user (uid 1000). A named volume can come up owned by root, or by whichever container mounted it first, so the `volume-init` one-shot (a) makes the three data directories writable for uid 1000 before `api` and `worker` start, and (b) every secondary mount of `output` (the gateway, a test runner) uses `nocopy: true`: without it, the first read-only mount copied another image's ownership into the empty volume and every document failed with `EACCES` at placement (found in CI run 5). RAN.

**Rollback (RAN, ops job).** Images are tagged by `IMAGE_TAG` (`doc-ts/api:<tag>`, `doc-ts/worker:<tag>`). To go back: `IMAGE_TAG=<previous> docker compose up -d --no-build api worker`. The volumes (documents, clones, the queue in Valkey) are the same, so an earlier document is still served and the queue is not lost; the `ops` CI job exercises exactly this switch (and back).

**Resource limits and what happens at them.** A container over its memory limit is killed by the engine and restarted by the restart policy; the job it was running is recovered as stalled by the restarted worker (the same path as a `kill -9`, below). The worker's own per-format ceilings (`PDF_MEMORY_CEILING_MB`, `DOCX_MEMORY_CEILING_MB`) kill a runaway Python process first and retry the job; Prince runs as a child process inside the worker container and counts against the container's limit.

**PDF engine.** `PDF_ENGINE=auto` (default): Prince when the request sets `use_prince` and the binary is installed, else WeasyPrint; a `PdfEngineFallback` finding says when Prince was wanted but not used. Prince is installed the way DOC's own Dockerfile installs it (the `.deb` from princexml.com, no licence file, so its non-commercial notice appears on the first page); on a CPU architecture without that package the image still builds and uses WeasyPrint.

**Failure drills (what to expect; all RAN in the `ops` CI job, run 9: https://github.com/linearcombination-ai/DOC/actions/runs/45732097074).**

| Drill | Command | Expected (measured) |
|---|---|---|
| SIGTERM the worker mid-job | `docker compose stop -t 130 worker` | log `shutting down: no new jobs, draining the running ones`, then `drained; bye`; **exit 0 after 11 s**; the job finishes; the document arrives |
| the worker process crashes mid-job (out-of-memory kill, segfault) | simulate with `crash worker '^node src/worker-main'` from `ci/ops.sh` (a `/proc` loop run with `docker exec`, no extra tools needed), or let the memory limit do it | the container exits non-zero and **`unless-stopped` restarts it** (RestartCount 0 to 1, healthy after 7 s); the job is recovered as stalled once its lock expires (`job stalled: its worker stopped renewing the lock`, about 30 to 45 s); the document arrives |
| `docker kill -s KILL` (or `docker stop`) on the worker | `docker kill -s KILL $(docker compose ps -q worker)` | **the engine treats this as a manual stop and does NOT restart the container**: start it again (`docker compose start worker`); the job is recovered as stalled and the document arrives (51 s end to end) |
| Valkey restart | `docker compose restart valkey` | AOF replay (8 s): earlier task records still answer, a job in flight finishes, new jobs work |
| API crash, then SIGTERM | `kill -9` inside as above, then `docker compose stop api` | restarted by the policy (healthy after 6 s); SIGTERM exits 0 in under a second; clients retry (the UI polls) |
| Roll to another image tag | `IMAGE_TAG=<tag> docker compose up -d --no-build api worker` | same volumes, same queue; earlier documents still served; switch back the same way |
| 24 documents at once | `python3 ci/loadgen.py burst http://localhost:5005 24` | 0 failed, 2.8 s |

To re-run all of them against your own stack: `cd platform-ts && OUT=out SOAK_MINUTES=2 bash ci/ops.sh` (needs the stack up and healthy; it kills and restarts services on purpose).

## 1. What runs (processes; the container map is in section 0)

```
client --> [gateway] --> API process   (node src/main.ts)        light wasm module, Hono, no job work
                              |  BullMQ over Redis (or Valkey)
                              v
                         Redis 8 / Valkey 9   queue, status, metrics counters, worker heartbeats, email records, clone locks
                              ^
                         worker process(es) (node src/worker-main.ts)   full wasm module on a worker thread for parse/assemble,
                              |                                          git clones, supervised Python docx pool
                              +--> python/docx_server.py x DOCX_POOL_SIZE   (or one python/docx_worker.py per job: DOCX_MODE=process)
output directory (DOCUMENT_OUTPUT_DIR) <- documents: <key>.docx, <key>.html, <key>.pdf, <key>.findings.json
```

Start order does not matter (the API answers 503 `queue_unavailable` until Redis is there; workers retry their connection). Stop with SIGTERM.

## 2. Start, stop, drain

| Task | Command |
|---|---|
| Local stack (Redis, catalog mock + git remotes, API, 1 worker) | `bash scripts/stack.sh start` ; `status` ; `logs api` ; `stop` |
| API only | `DATA_API_URL=... REDIS_URL=... node src/main.ts` |
| Worker | `DATA_API_URL=... REDIS_URL=... node src/worker-main.ts` |
| Graceful stop of a worker (drain) | `kill -TERM <pid>`: it takes no new job, finishes the running ones, exits 0. Deadline `SHUTDOWN_GRACE_MS` (default 120 s): after it the process logs `drain deadline passed: abandoning running jobs` and exits 1; BullMQ gives the abandoned jobs to another worker after `LOCK_DURATION_MS` (stalled recovery) |
| Graceful stop of the API | `kill -TERM <pid>`: stops listening, finishes in-flight requests, exits 0; deadline `API_DRAIN_MS` (default 10 s), then exit 1 |
| Before a restart: what is running | `node tools/queue-admin.ts drain-status` |
| Rolling restart | start the new worker, wait for `doc_workers_alive` to rise, SIGTERM the old one, wait for exit 0 |

Exit codes: 0 clean, 1 drain deadline passed, **78 invalid configuration** (the first stderr line is a JSON log record with `problems`, then one readable line per problem: fix them all, restart). RAN: `test/config.test.ts`.

## 3. Configuration (all environment variables; validated at start)

Validation collects every problem and prints the variable, the reason and the value given (secrets are not echoed). RAN. Beyond type checks it refuses combinations that cannot work (a stage deadline longer than the job deadline, retention shorter than the cache window, `SEND_EMAIL` without `SMTP_HOST`, a TLS certificate without its key, and so on) and, before taking traffic, checks that directories can be written, the wasm core is built, `PYTHON_BIN` runs Python 3.12 or newer, the docx scripts exist and `git` runs.

| Group | Variables (defaults) |
|---|---|
| Catalog | `DATA_API_URL` (required), `DATA_API_CACHE_TTL_SECONDS` 180, `DATA_API_TIMEOUT_MS` 15000, `DATA_API_MAX_ATTEMPTS` 5, `DATA_API_DEADLINE_MS` 60000 |
| Repositories | `RESOURCE_ASSETS_DIR` assets_download, `ASSET_CACHING_PERIOD` 30 (minutes), `GIT_CLONE_TIMEOUT_MS` 180000, `GIT_MAX_ATTEMPTS` 4, `CLONE_LOCK` true, `CLONE_LOCK_LEASE_MS` 30000 |
| Queue | `REDIS_URL` redis://127.0.0.1:6379/0, `QUEUE_PREFIX` docp6, `QUEUE_NAME` documents, `JOB_ATTEMPTS` 4, `JOB_BACKOFF_BASE_MS` 2000, `JOB_BACKOFF_CAP_MS` 60000, `LOCK_DURATION_MS` 30000, `STALLED_INTERVAL_MS` 15000, `MAX_STALLED_COUNT` 3, `WORKER_CONCURRENCY` 2 |
| Deadlines | `JOB_DEADLINE_MS` 900000; stages: `STAGE_LOCATE_DEADLINE_MS` 120000, `STAGE_PROVISION_DEADLINE_MS` 600000, `STAGE_PARSE_DEADLINE_MS` 120000, `STAGE_ASSEMBLE_DEADLINE_MS` 120000, `STAGE_DOCX_DEADLINE_MS` 300000 |
| Docx seam | `DOCX_MODE` persistent \| process, `DOCX_POOL_SIZE` 2, `DOCX_RECYCLE_AFTER` 200 jobs, `DOCX_MEMORY_CEILING_MB` 400, `PYTHON_BIN` python3 (WeasyPrint needed in it for PDF), `PDF_ENGINE` weasyprint \| off |
| Output | `DOCUMENT_OUTPUT_DIR`, `WORKING_DIR`, `OUTPUT_TTL_MINUTES` 30 (DOC's cache window: a younger document is served as is), `OUTPUT_RETENTION_MINUTES` 1440 (older files deleted by the sweeper; 0 = never), `DISK_MIN_FREE_MB` 200 |
| Limits | `MAX_BODY_BYTES` 262144, `MAX_RESOURCE_REQUESTS` 200, `MAX_BOOK_CODES` 100; language codes 1 to 35 letters, digits, hyphens (fixed mode) |
| Email | `SEND_EMAIL` false, `SMTP_HOST`, `SMTP_PORT` 25, `SMTP_STARTTLS` auto \| require \| off, `SMTP_USER`, `SMTP_PASSWORD`, `FROM_EMAIL_ADDRESS`, `EMAIL_SEND_SUBJECT`, `EMAIL_ON_CACHE_HIT` true, `EMAIL_RECORD_DAYS` 14 |
| Shutdown | `SHUTDOWN_GRACE_MS` 120000 (worker drain deadline), `API_DRAIN_MS` 10000 |
| Health | `QUEUE_LAG_WARN_MS` 60000, `CATALOG_STALE_WARN_SECONDS` 3600 |

### Redis: authentication and TLS (RAN against a TLS-enabled Redis 8.12.0 built from source, `test/config.test.ts`)

* Password: `REDIS_PASSWORD` (and `REDIS_USERNAME` for an ACL user) or the password in `REDIS_URL`. The environment variable keeps it out of the URL (and therefore out of logs and `ps`); the startup log line `job queue configured` prints host, port, db, `tls`, `auth: true|false`, never the secret.
* TLS: `REDIS_URL=rediss://host:6380/0` or `REDIS_TLS=true`; `REDIS_TLS_CA` (PEM file of the CA that signed the server certificate), `REDIS_TLS_CERT` + `REDIS_TLS_KEY` for mutual TLS, `REDIS_TLS_SERVERNAME` when the certificate name differs from the host. Verification is on; `REDIS_TLS_REJECT_UNAUTHORIZED=false` switches it off and the service warns at every start.
* Server side: Redis must be built or packaged with TLS (`BUILD_TLS=yes`; the binaries used here were) and started with `--tls-port`, `--tls-cert-file`, `--tls-key-file`, `--tls-ca-cert-file`.

### Redis persistence settings (documented; the harness uses the first two)

| Setting | Value | Why |
|---|---|---|
| `appendonly yes`, `appendfsync everysec` | on | a crash or `kill -9` loses at most about one second of queue writes. RAN: the Redis `SIGKILL` + restart test, job completed once afterwards |
| `maxmemory-policy noeviction` | required | BullMQ keys must never be evicted; `/health/status` reports `redis.evictionPolicy` and names a different policy as a problem |
| `save ""` (RDB off) or `save 900 1` | optional | an RDB snapshot beside the AOF shortens restarts; not needed for correctness |
| `maxmemory` | size for the retained job records | completed and failed job records are kept `KEEP_JOBS_SECONDS` (24 h); at tens of thousands of jobs a day that is megabytes, not gigabytes (INFERRED from key sizes, not measured at scale) |
| `bind` / firewall | internal only | auth and TLS above protect a shared network |

Licences read from the source trees built here: **Redis 8.12.0** is tri-licensed (`LICENSE.txt`: RSALv2, SSPLv1 or AGPLv3, at your choice; the 7.2 and older lines were BSD-3); **Valkey 9.2.0** is BSD 3-Clause (`COPYING`). The shell talks plain Redis protocol and its tests pass on both (section 9 of COMPARISON). RSALv2 forbids offering the software as a managed service to third parties, which DOC does not do; whether the licence is acceptable is the project's call (INFERRED reading, not legal advice).

## 4. Symptoms and what to do

| Symptom | Likely cause | Check | Action |
|---|---|---|---|
| Clients poll `PENDING` forever | the task id is unknown (Redis lost its data, or a mistyped id: Celery's behaviour, kept) | `tools/queue-admin.ts show <id>` says `no job` | resubmit the request; section 7 for a Redis loss |
| `POST /documents_docx` answers 503 `queue_unavailable` | Redis unreachable from the API | `/health/status` `redis.up false`; `doc_redis_up 0` | restore Redis; no action on the API (it reconnects) |
| 503 `disk_space_low` on POST | free space under half of `DISK_MIN_FREE_MB` | `/health/status` `disk` | free space; sweep `DOCUMENT_OUTPUT_DIR`; lower `OUTPUT_RETENTION_MINUTES` |
| Jobs wait, nothing runs | no worker alive | `doc_workers_alive 0`, health problem "no worker has a fresh heartbeat" | start a worker; look at its log for exit 78 |
| Jobs fail with `catalog_unavailable` after retries | the data API is down longer than the retry window | `doc_catalog_refreshes_total{result="failed"}`, log `catalog refresh failed` | wait; failed jobs can be replayed (section 6); the last good catalog is still served to lookups (`x-doc-stale: 1`) |
| Jobs fail with `resource_unavailable` | a repository could not be cloned (host down) or the book is not in it | log `repository could not be fetched` / `has no USFM file for` | permanent (`retryable:false`) when the book is missing; transient for network errors |
| `stage_deadline` / `docx_worker_timeout` | a stage ran past its deadline | log `stage finished ok=false`, `docx worker killed` | raise the stage deadline if the content is legitimately big; otherwise investigate the host |
| `docx_worker_memory` | a Python worker passed `DOCX_MEMORY_CEILING_MB` mid-job | log `persistent docx worker killed and replaced` with `stop: memory` | raise the ceiling or find the document that needs it |
| `job_stalled` failures | workers repeatedly died holding the job (OOM kill, host restarts) | `doc_stalled_recoveries_total` rising, worker logs | find why the worker dies; a single document that kills workers is failed after `MAX_STALLED_COUNT` |
| Email not received, job succeeded | finding `EmailNotSent` (server refused) or `EmailOutcomeUnknown` | `GET /findings/documents/<key>` | see section 8 |
| Memory of a worker grows | persistent Python workers keep fonts and caches | `doc_worker_rss_bytes`, `/health/status` `docxPool` | lower `DOCX_RECYCLE_AFTER`, or `DOCX_MODE=process` |
| API logs `invalid configuration` and exits 78 | see the listed problems | stderr | fix and restart |

## 5. Log lines (pino JSON on stdout; every line has `service`, `time`, `level`; 30 info, 40 warn, 50 error)

One request is one `reqId` from the HTTP request to the worker to the Python process: `grep '"reqId":"<id>"'` (the id is the caller's `x-request-id` if it is 1 to 64 plain characters, else generated; it is echoed in the response header and in email headers). Python lines carry `req_id` and `msg` prefixed `python:`.

| `msg` | Level | Meaning and fields |
|---|---|---|
| `response` | info | one per HTTP request: `method`, `path`, `status`, `ms`, `reqId` |
| `job enqueued` / `joined an identical in-flight job` | info | `job`, `key`, `deduplicated`; identical means same document, same PDF flag, same recipient |
| `attempt started` | info | `job`, `attempt`, `of`, `key`, `kind`, `queuedMs` (time waited in the queue, first attempt) |
| `stage` | info | the state a poller sees changed (`stage`) |
| `stage finished` | info | one per stage and attempt: `stage`, `ms`, `ok` |
| `document worker finished` | info | `format`, `mode`, `pid`, `wallMs`, `workerMs`, `rssKb`, `bytes` |
| `python: render start` / `python: render done` | info | from the Python process: `req_id`, `job`, `format`, `bytes`, `ms` |
| `document placed` | info | files written, `findings`, `timings` |
| `job finished` | info | `ms`, `cacheHit`, `findings`, `timings` (all stages) |
| `attempt failed, will retry` | warn | `code`, `transient true`, `attempt`, `of`, `err`, `detail`; the queue will retry with backoff |
| `attempt failed, giving up` | warn | `code`; permanent, or attempts exhausted: the client sees `FAILURE` with `{code,message,retryable}` |
| `job stalled: its worker stopped renewing the lock` | warn | a worker died or hung holding `job`; another worker takes it |
| `repository cloned` | info | `url`, `attempts`, `ms`, `fence` (the lock's fencing number) |
| `another worker published this repository while we waited` | info | the clone lock worked: no second clone |
| `clone lock unavailable (Redis); cloning without it` / `waited for the clone lock past the limit` | warn | degraded but correct (a duplicate clone may happen) |
| `this clone was fenced out by a newer generation` | warn | a worker whose lease expired found a newer copy and kept it |
| `persistent docx worker killed and replaced` | warn | `stop` deadline, abort or memory; the job fails transient and is retried on a fresh process |
| `persistent docx worker exited on its own` | warn | a crash; replaced |
| `recycling the persistent docx worker` | info | after `DOCX_RECYCLE_AFTER` jobs or over the memory ceiling |
| `catalog refreshed` / `catalog refresh failed; serving the last good answer` | info / warn | the catalog client; `ms`, `rows` |
| `not sending: this email was already sent, is being sent, or its outcome is unknown` | warn | the email record did its job; `state` sent, sending or unknown |
| `email sent` / `the mail server did not take the message` / `the outcome of the send is unknown` | info / warn / error | section 8 |
| `removed scratch directories left by dead workers` | warn | at worker start; `removed` lists them |
| `removed expired documents` | info | the sweeper; `removed`, `retentionMinutes` |
| `shutting down: ...` / `drained; bye` / `drain deadline passed` | info / error | graceful shutdown (section 2) |

## 6. How to replay a failed job

A failed job is one that exhausted its attempts or failed permanently; its record is kept `KEEP_JOBS_SECONDS`.

```
node tools/queue-admin.ts failed 20                # id, key, request id, code, attempts, when
node tools/queue-admin.ts show <id>                # data, progress, failure result, stack
node tools/queue-admin.ts replay <id>              # same data, attempts reset; the job goes back to waiting
node tools/queue-admin.ts replay-all-failed        # after fixing a shared cause (catalog down, disk full)
```

Replaying is safe: the document is a pure function of the request (same key, same bytes, placed atomically) and the email step has its own record, so a replay cannot send an email that was already sent. A permanent failure (`retryable:false`, for example a book the repository does not have) fails again until the cause is fixed. The client that submitted the original job polls the old id, which stays `FAILURE`; replay serves everyone else asking for the same document, and the file is in the output directory. Equally valid: ask the user to submit again.

## 7. Recovering from a Redis loss

What Redis holds: the queue (waiting, delayed, active, failed, completed records), the status strings, the metrics counters, worker heartbeats, clone locks and email records. What it does not hold: documents (output directory) and repositories (asset directory).

| Situation | What happens | Do |
|---|---|---|
| Redis restarts with its AOF (normal restart, `kill -9`) | at most about one second of writes lost; the API answers 503 meanwhile; workers and the API reconnect by themselves; running jobs finish and are recorded once Redis is back. RAN (stop/restart and SIGKILL/restart tests) | nothing; check `doc_redis_up` |
| Redis restarts empty (AOF lost) | all queued and running jobs are gone, polling clients see `PENDING` for their old ids, counters restart at 0, email records are gone | 1. start Redis, workers reconnect on their own (`doc_workers_alive`). 2. tell users to resubmit (their UI polls forever otherwise: the unknown-id-is-PENDING quirk of Celery is kept). 3. documents already in the output directory are served as cache hits. A worker killed in this window leaves scratch directories, removed at the next worker start |
| Wrong persistence (eviction on) | BullMQ keys disappear under memory pressure | set `maxmemory-policy noeviction`; health reports it |
| Switch Redis to Valkey (or back) | same protocol, same keys | stop workers (drain), start the other server on the same AOF directory or empty (above), start workers. The Valkey run of the whole suite is in `results/suite-valkey9.txt` |

## 8. Email

* Recipients come from the request's `email_address`; sent only with `SEND_EMAIL=true` and `SMTP_HOST`; otherwise a finding `EmailDisabled` says so.
* One record per (submission, recipient, document) in Redis, written before the send. Retries and redeliveries find it and do not send again (RAN: `test/email.test.ts`, including 20 concurrent attempts sending one message).
* Findings on a finished document: `EmailNotSent` (the server did not take the message, definite: a later attempt may send; the claim was released), `EmailOutcomeUnknown` (the connection failed after the body was sent: it will **not** be sent again; check the mail log and, if you decide it was lost, `node tools/queue-admin.ts forget-email <job id>` then replay).
* A process killed between the server accepting the message and the record turning to `sent` leaves the record at `sending`: a redelivery skips the send. At most once, on purpose; the document is unaffected.

## 9. Metrics (Prometheus text at `GET /metrics`, fleet-wide; scrape one API process)

Counters live in Redis (`<prefix>:metrics`), so they reset if Redis loses its data (treat as a counter reset); gauges are read at scrape time.

| Metric | Alarm on |
|---|---|
| `doc_queue_jobs{state}` | `waiting` above your worker capacity for 5 min; `failed` rising |
| `doc_queue_oldest_waiting_seconds` | above 60 for 5 min: jobs are waiting too long (workers down or slow) |
| `doc_workers_alive`, `doc_worker_heartbeat_age_seconds`, `doc_worker_running_jobs` | `doc_workers_alive` below the number you run; heartbeat age above 15 s |
| `doc_worker_rss_bytes` | growing steadily across hours (a leak); near the host limit |
| `doc_job_attempts_total{result}` | `retry` and `failed` as a share of `ok` |
| `doc_job_failures_total{code,retried}` | any `retried="false"` code other than `resource_unavailable` (permanent request errors); `catalog_unavailable`, `stage_deadline`, `docx_worker_*` bursts |
| `doc_stage_duration_seconds` (histogram per stage) | p95 of a stage doubling against its baseline |
| `doc_job_duration_seconds` | p95 end to end |
| `doc_stalled_recoveries_total` | any increase: a worker died or hung holding a job |
| `doc_repo_clones_total{outcome}` | `lock_unavailable` rising (Redis trouble); `failed` rising (content host trouble) |
| `doc_repo_cache_hits_total` | a fall after a deploy (asset directory lost) |
| `doc_catalog_refreshes_total{kind,result}`, `doc_catalog_last_success_timestamp_seconds` | `failed` or `stale_served` rising; last success older than the TTL times 20 |
| `doc_docx_worker_events_total{event,mode}` | `crash`, `timeout`, `memory_kill` rising; `recycled` is normal |
| `doc_docx_jobs_total{mode,result}` | non-`ok` results |
| `doc_docx_pool_workers{state}` | `starting` stuck above 0 |
| `doc_email_total{result}` | `unknown` or `failed` rising |
| `doc_redis_up` | 0 |
| `doc_http_requests_total{route,status}` | 5xx share |
| `doc_metrics_dropped_total` | above 0: increments could not be sent to Redis (counters undercount) |

Suggested alert, as an example (not tested against a Prometheus server here): `doc_workers_alive < 1 or doc_queue_oldest_waiting_seconds > 60 or doc_redis_up == 0`.

## 10. `/health/status`

`GET /health/status` answers `[{status, ...}, 200]` (FastAPI's tuple quirk, kept) and always HTTP 200 so existing liveness probes keep working; `?strict=1` answers 503 with the same body when `status` is `degraded`. Body: `redis` (up, version, latency, memory, AOF, last save age, eviction policy, clients), `queue` (counts, oldest waiting), `workers` (id, pid, heartbeat age, running jobs, rss), `docxPool` (mode, size, idle, busy, recycled, crashed), `catalog.lastSuccessAgeSeconds`, `disk` (free MB and floor), and `problems` (one sentence each) when degraded.

## 11. Disk

Scratch directories are per attempt under `WORKING_DIR/jobs/<pid>-<job>-<attempt>-<random>` and are removed when the attempt ends; a worker removes those of dead pids when it starts. Documents live in `DOCUMENT_OUTPUT_DIR` for `OUTPUT_RETENTION_MINUTES`; the sweeper (every `OUTPUT_SWEEP_INTERVAL_MS`) deletes older `.docx`, `.html`, `.pdf`, `.json` and partial files. Repositories live in `RESOURCE_ASSETS_DIR` (generations replaced atomically, old ones removed after a grace period); this directory is not swept by the shell (DOC's own sweep behaviour is a separate question). Below `DISK_MIN_FREE_MB` a job waits and retries (`disk_space_low`); at half of it the API refuses new jobs.

## 12. Beside FastAPI behind a path-prefix gateway (INFERRED: not run, no docker here)

DOC's endpoints are the same paths (`/documents`, `/documents_docx`, `/task_status/{id}`, the lookups, `/health/status`), so the gateway can send selected paths to this service while the rest stays on FastAPI, and flip them one at a time:

```nginx
upstream doc_fastapi { server api:5005; }
upstream doc_ts      { server doc-ts-api:18301; }
server {
  listen 80;
  # new service first for the document endpoints; everything else (STET, Passages, lookups) stays on FastAPI until ported
  location ~ ^/(documents|documents_docx|task_status/|findings/|metrics$) { proxy_pass http://doc_ts; proxy_set_header X-Request-Id $request_id; }
  location / { proxy_pass http://doc_fastapi; proxy_set_header X-Request-Id $request_id; }
}
```

Both services write to the same `DOCUMENT_OUTPUT_DIR` volume that the file server serves and use the same cache key function (RAN in P5: the Rust core's key equals DOC's), so a document built by either is a cache hit for the other. They must not share a Redis database or key prefix with Celery: this service uses `QUEUE_PREFIX` (default `docp6`) and BullMQ keys of its own. The shell needs: node 26 (or 24), the built wasm (`core/pkg`, `core/pkg-api`), Python 3.12+ for the docx seam (with WeasyPrint for PDF), git, and a Redis 8 or Valkey 9. A container image would be one node image plus Python and git (not built here).
