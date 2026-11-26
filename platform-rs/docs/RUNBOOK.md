# Runbook: `doc-jobd` (Rust shell, P6 and P7)

For whoever is on call. Everything here was exercised by the test suite or the commands in `README.md` unless it says
**not exercised**. Paths are relative to `p6rs/`; `redis-cli` can be `valkey-cli` (same protocol).

## 0. The container deployment (P7)

Everything in sections 1 to 8 below describes `doc-jobd` itself and still holds; this section is how it runs as a stack of containers from
`docker-compose.yml` (compose project name `doc-rs`). Evidence tags as in `COMPARISON.md`; what CI ran is in its "Docker and CI" section.

```
browser -> :8001 frontend  (DOC's own SvelteKit image, built from frontend/Dockerfile unchanged)
        -> :5005 gateway --> api:8081     (doc-jobd, DOC_JOBD_ROLE=api)        :8089 gateway --> output volume (read-only, DOC's file URLs)
api, worker <--> valkey:6379 (AOF everysec, noeviction, volume valkey-data)
worker (doc-jobd, DOC_JOBD_ROLE=worker) --> git clones (volume assets) --> core child process --> HTTP --> pyworker:9100 (Python docx + PDF: Prince, WeasyPrint fallback, fonts)
volumes: output (/app/document_output, shared by api, worker, pyworker, gateway), assets (git clones: the 30-minute cache), valkey-data
```

| Service | Image | Memory limit | CPUs | Restart | Stop grace | Health check |
|---|---|---|---|---|---|---|
| `valkey` | `valkey/valkey:9.2.0-alpine` | 256 MB | 1 | unless-stopped | 20 s | `valkey-cli ping` |
| `pyworker` | `doc-rs/pyworker` (python 3.13, Prince 16.1, WeasyPrint, fonts) | 3 GB | 2 | unless-stopped | 150 s | `GET /readyz` |
| `api` | `doc-rs/core` (Rust, 187 MB) | 768 MB | 1 | unless-stopped | 30 s | `GET /health/status` |
| `worker` | `doc-rs/core` (same image, other role) | 1 GB | 2 | unless-stopped | 180 s | `GET :8082/health/status` |
| `gateway` | `doc-rs/gateway` (nginx unprivileged) | 128 MB | 0.5 | unless-stopped | default | `/gateway/health` on both ports |
| `frontend` | `doc-rs/frontend` (DOC's) | 384 MB | 1 | unless-stopped | default | `GET /health` |

All limits can be overridden by environment variables (`.env.example`). The application containers run as uid 10001 (non-root); the gateway is nginx-unprivileged;
`init: true` gives every container a reaper. Only 8001, 5005 and 8089 are published.

**Commands** (from `platform-rs/`):

| Task | Command |
|---|---|
| Start, building what is missing | `docker compose up --build -d` (wait for `docker compose ps` to show every service healthy; about 30 s after the images exist) |
| State, health | `docker compose ps`; `curl localhost:5005/health/status` (API and queue), `curl localhost:5005/metrics` |
| Logs of one service, one request | `docker compose logs -f worker`; `docker compose logs worker pyworker \| grep '"request_id":"<id>"'` |
| More workers | `docker compose up -d --scale worker=3` (each one claims jobs from the same queue; `pyworker` is the bottleneck for PDF: raise `PYWORKER_CONCURRENCY` and its memory limit together) |
| Graceful stop of one service | `docker compose stop worker` (SIGTERM: the running jobs finish for up to 120 s, then are handed back to the queue; exit code 0) |
| Everything down, data kept / removed | `docker compose down` / `docker compose down -v` |
| Roll the application back | `docker tag doc-rs/core:<good tag> doc-rs/core:rollback && DOC_RS_TAG=rollback docker compose up -d --no-build api worker` (same volumes, same Valkey: earlier documents are still served and queued jobs survive); `DOC_RS_TAG=<normal tag> docker compose up -d --no-build api worker` goes back |
| Look inside Valkey | `docker compose exec valkey valkey-cli` (keys under `docjobs:`) |

**What the engine does by itself (RAN in CI, `ops` job; numbers in `COMPARISON.md`):** a crash inside a container (an out-of-memory kill, a segfault) is
restarted by `restart: unless-stopped` and the interrupted job is recovered (stalled-job path, a retry); `docker kill` and `docker stop` are *manual stops*
and are **not** restarted (start the service again); Valkey restarts with its append-only file and queued, running and finished jobs are intact; the API
answers a typed 503 while Valkey is away and recovers without a restart. A PDF request that Prince cannot make (missing, crashed, hung) is made with
WeasyPrint and the document's findings file (`<key>.findings.json`) carries `pdf_engine_fallback`.

**Prince and its licence.** `doc-rs/pyworker` installs the Prince `.deb` exactly as DOC's own Dockerfile does (same version, same source). There is no licence
file, so Prince runs in its non-commercial mode and puts its notice on the first page, as DOC's PDFs do today. Where Prince has no package for the image's CPU
architecture the build does not fail: the service makes PDFs with WeasyPrint and the findings file says so. Installing a licence file is the captain's decision
(not built here, INFERRED: Prince reads `license.dat` from its `license/` directory).

**Not exercised in containers:** a container hitting its memory limit (the engine's OOM kill), CPU throttling, multi-host, arm64 images (CI built the
x86_64 Prince package in CI; the Dockerfile asks for the package of its own architecture and falls back to WeasyPrint when there is none, which CI did not exercise), registry pushes, image scanning.

## 1. What runs

One binary, `doc-jobd`, with a role (`DOC_JOBD_ROLE`): `all` (API and worker, the default), `api` or `worker`. The API
accepts requests and enqueues; workers run the jobs; they meet only in Redis (or Valkey). Any process can be restarted,
scaled or killed without the others noticing. A worker runs, per job: *locate* (catalog) -> *provision* (git clones, one
per repository across all workers) -> *core* (parse and assemble, in a killable child process) -> *docx* (a Python process,
per job or from a persistent pool) or *html* -> placement of the file in the output directory -> the optional *email*.

```
client -> [API] -> Redis stream -> [worker] -> catalog, git, core child, Python docx worker -> output dir -> fileserver
                       ^ counters, leases, locks, email records, process registry
```

## 2. Configuration (validated at start)

Every setting is read once, at start. A wrong value makes the process print **every** problem and exit with status 2
before it binds a port or touches Redis (`doc-jobd check-config` does only that: it prints a summary with secrets
redacted, status 0, or the problems, status 2). DOC's own names are kept where DOC has them.

| Variable | Default | Meaning |
|---|---|---|
| `DATA_API_URL` | required | The BIEL GraphQL endpoint. |
| `DOC_JOBD_ROLE` | `all` | `all`, `api`, `worker`. |
| `DOC_JOBD_STORE` | `redis` | `redis` (Valkey too) or `memory` (one process, nothing survives a restart; tests and local development). |
| `DOC_JOBD_REDIS_URL` (fallback `CELERY_BROKER_URL`) | `redis://127.0.0.1:6379/0` | `rediss://` for TLS. |
| `DOC_REDIS_PASSWORD` / `DOC_REDIS_PASSWORD_FILE`, `DOC_REDIS_USERNAME` | none | Credentials kept out of the URL (the URL is redacted in logs). |
| `DOC_REDIS_CA_FILE` | system roots | PEM root to trust for a private CA (needs `rediss://`). |
| `DOC_JOBD_NAMESPACE` | `docjobs` | Key prefix; several services can share one Redis. |
| `DOC_SHELL_ADDR` | `127.0.0.1:8081` | Listen address of the API. |
| `DOC_SHELL_MODE` | `fixed` | `fixed` (production) or `as_built` (comparison with DOC). |
| `DOC_JOBD_OUTPUT_DIR` | `document_output_rs` | Where documents (`<key>.docx`, `<key>.html`, `<key>.findings.json`) are placed. Scratch is `.scratch/` inside it. |
| `ASSET_CACHING_PERIOD` | `30` (minutes) | DOC's window: a document younger than this is served again, a repository copy younger than this is reused. |
| `DOC_JOBD_OUTPUT_GRACE_MS` | 1800000 | Outputs older than the window plus this are deleted by the janitor (`DOC_JOBD_JANITOR_EVERY_MS`, 300000). |
| `DOC_JOBD_CONCURRENCY` | `2` | Jobs at once per worker process. |
| `DOC_JOBD_LEASE_TTL_MS` | `60000` | Time a dead worker's job stays invisible before another worker takes it (recovery latency; also the longest tolerated stall of the heartbeat). Must exceed `DOC_JOBD_REAP_EVERY_MS` (5000). |
| `DOC_JOBD_MAX_ATTEMPTS` | `8` | Attempts per job; backoff is jittered between `DOC_JOBD_BACKOFF_BASE_MS` (5000) and `DOC_JOBD_BACKOFF_CAP_MS` (300000). |
| `DOC_JOBD_JOB_CEILING_MS` | `1800000` | One attempt's ceiling over all stages. |
| `DOC_JOBD_STAGE_{LOCATE,PROVISION,PARSE,ASSEMBLE}_MS` | 300000 / 900000 / 300000 / 300000 | Per-stage deadlines (generous: a request should end with a document). |
| `DOC_JOBD_DRAIN_DEADLINE_MS` | `120000` | On SIGTERM: how long running jobs may finish before they are handed back to the queue. |
| `DOC_DOCX_MODE` | `per_job` | `per_job` (a Python process per document), `persistent` (a pool, `DOC_DOCX_POOL_SIZE` = concurrency) or `remote` (an HTTP call to the Python service at `DOC_PYWORKER_URL`: the container deployment). `DOC_PDF_ENGINE` = `none`, `weasyprint`, `prince` (child processes) or `remote` (same service). |
| `DOC_BOOK_NAME_RULE` | `written` | `written` keeps the book names translators wrote (a finding says what DOC's rule would show); `doc` applies DOC's normalisation (`2ኛ ቆሮንቶስ` becomes `2 ኛ ቆሮንቶስ`), which DOC's own UI test for Amharic reads. |
| `DOC_DOCX_RECYCLE_AFTER` / `DOC_DOCX_RSS_CEILING_MB` | 200 / 512 | A pooled worker is replaced after N jobs or when its resident size passes the ceiling. |
| `DOC_JOBD_DOCX_DEADLINE_MS` | `300000` | One docx job; past it the worker and its process group are killed. |
| `DOC_CORE_MODE` | `child` | `child` (killable process, `DOC_JOBD_CORE_DEADLINE_MS`) or `inline` (blocking pool; not cancellable). |
| `DOC_JOBD_MAX_BODY_BYTES` | `65536` | Larger request bodies get 413. |
| `DOC_JOBD_MAX_RESOURCES` | `100` | Most resource requests per document request (422 above). Language codes are 1 to 35 letters, digits, hyphens (422 otherwise). |
| `DOC_JOBD_MIN_FREE_MB` | `512` | Below this much free space on the output filesystem new requests get a typed 503 `disk_space_low` and running jobs fail transiently (retried). |
| `DOC_SHELL_CLONE_LOCK_LEASE_SECS` / `..._WAIT_SECS` | 30 / 660 | The cross-process clone lock. `DOC_JOBD_CLONE_LOCK=false` turns it off (a test control). |
| `SEND_EMAIL`, `SMTP_HOST`, `SMTP_PORT`, `SMTP_TLS` (`none`/`starttls`/`tls`), `SMTP_USER`, `SMTP_PASSWORD[_FILE]`, `FROM_EMAIL_ADDRESS`, `EMAIL_SEND_SUBJECT`, `SMTP_CA_FILE` | disabled | The email step. Credentials with `SMTP_TLS=none` are refused at start. |
| `DOC_TEMPLATES_DIR` | `../../../backend/templates/html` relative to the binary | DOC's own template files for the HTML header (checked at start). |
| `DOC_JOBD_OPS_ADDR` | none | A `worker`-only process serves `/health/status` and `/metrics` here. |
| `DOC_JOBD_BEAT_EVERY_MS` | `5000` | Each process reports itself to Redis; three missed beats = down. |
| `RUST_LOG`, `DOC_SHELL_LOG_FORMAT=json` | `info`, text | Log level and format. |

## 3. Redis (or Valkey) persistence and security

* **Persistence.** Run with `appendonly yes` and `appendfsync everysec` (what the tests and the demo use): a restart or
  a crash loses at most the last second of state; queued jobs and running jobs survive. Snapshots (`save`) are not
  needed. DOC's own compose file sets no persistence today. Without persistence a Redis restart loses every queued job
  and every job's state (section 8); documents already on disk stay valid.
* **Memory.** The state is small (a job hash of a few KB, expiring after 24 h; the histogram counters). `maxmemory-policy noeviction`.
* **Auth and TLS.** `requirepass` (or ACL users) plus `DOC_REDIS_PASSWORD`; TLS with `tls-port`, `tls-cert-file`,
  `tls-key-file`, `tls-ca-cert-file` and `rediss://...` with `DOC_REDIS_CA_FILE` for a private CA. All of it is covered by
  `redis_password_and_tls_are_supported`. Servers must be built with `BUILD_TLS=yes` (`scripts/build-servers.sh` does).
* **Redis and Valkey** both pass the whole suite (section 9 of `COMPARISON.md`).

## 4. Starting, stopping, draining

* **Start:** `doc-jobd` with the environment above. It waits for Redis if Redis is not up yet (it does not exit). On start a
  worker removes scratch directories older than two job ceilings (dead attempts) and logs how many.
* **Stop / drain:** `kill -TERM <pid>`. The process stops claiming jobs, lets running jobs finish for up to
  `DOC_JOBD_DRAIN_DEADLINE_MS` (log line `draining`), then cancels what is left: the Python worker and the core child are
  killed and the job is **handed back to the queue without counting an attempt** (`drain deadline: handing the job back
  to the queue`); another worker, or this one after a restart, finishes it. The process exits 0. Set the platform's stop
  grace period above the drain deadline plus about 30 s. SIGKILL is also safe, at the price of the lease wait (section 6).
* **Rolling restart:** stop one process at a time; the others keep claiming.

## 5. Logs: what each line means

JSON lines with `DOC_SHELL_LOG_FORMAT=json` (fields at the top level). **Every job line carries `request_id`** (the HTTP
request's `X-Request-Id`, or one made by the API; echoed in the response header), also the lines the Python process writes.
Follow one request: `grep '"request_id":"<id>"' log`.

| Message | Meaning |
|---|---|
| `request` | One HTTP request (method, path, status, ms). Health and metrics scrapes are not logged. |
| `document request accepted` | A job exists: `task_id`, `key`, `kind` (docx/html), `coalesced=true` if it joined a running identical request. |
| `attempt started` | A worker claimed the job (`attempt` n of `max`). One line per attempt. |
| `stage finished` / `stage failed` | One line per stage per attempt: `stage` (locate, provision, core, docx), `ms`, `ok`, `error`. |
| `cache hit: a fresh document exists` | The output was younger than the window; nothing was rebuilt. |
| `python docx worker` | Relayed from the Python process: `event` docx_start/docx_done/docx_failed/docx_rejected, `pid`, `worker_ms`. |
| `docx worker finished` / `(persistent)` | Wall time, worker time, `maxrss_kb`/`rss_kb`, `jobs_done` of the pooled worker. |
| `document placed` | The file is in the output directory (atomic rename); `total_ms`. |
| `attempt succeeded; job done` | Terminal success. |
| `attempt failed (transient); will retry` | `code`, `error`, `retry_in_ms`: the job is parked and comes back. The UI never sees FAILURE for this. |
| `attempt failed; job failed` | Terminal failure: `code`, `retryable`, `exhausted` (attempts used up). The status answer is `FAILURE` with `{code, message, retryable}`. |
| `lease lost: another worker owns this job now` | This worker was stalled past the lease; it stops and writes nothing (fencing). |
| `stalled jobs (worker lost) queued again` | The reaper found jobs whose worker died; counted in `doc_stalled_recoveries_total`. |
| `persistent docx worker replaced` | `reason` crash, deadline, recycle or memory (also `doc_docx_pool_restarts_total`). |
| `core stage killed (process group)` | A hung or runaway core child was killed at its deadline; the attempt is retried. |
| `repository fetched` / `clone attempt failed, will retry` / `clone failed, giving up` | One line per clone attempt. |
| `clone lock unavailable; cloning without it` | Redis could not grant the lock; the clone proceeds (a duplicate clone is only wasted work). |
| `catalog refreshed` / `catalog attempt failed, will retry` / `serving the stale catalog` | Data API health. |
| `email sent` / `email not sent` / `the email may or may not have been delivered...` | The email step (section 7). |
| `janitor removed files` | Expired outputs or dead scratch removed. |
| `store unavailable while claiming; will retry`, `cannot reach Redis; retrying` | Redis is down; the process waits with backoff and recovers by itself. |

Error codes in `attempt failed` and in the FAILURE result: transient (retried): `catalog_unavailable`, `repo_unavailable`,
`docx_worker_timeout`, `docx_worker_crashed`, `docx_worker_failed`, `docx_output_invalid`, `core_timeout`, `core_crashed`,
`stage_timeout`, `job_ceiling`, `disk_space_low`, `output_io`, `scratch_io`, `email_retry`, `internal_panic`; permanent:
`resource_unavailable` (every requested book is absent from the repository that holds it; a request whose language or resource the catalog does not have is *not* an error: it is left out of the document and `resource_not_in_catalog` is in the findings), `book_not_found`, `invalid_request`, `docx_input_rejected`, `core_rejected`, `resource_unreadable`,
`usfm_unreadable`, `worker_lost_repeatedly` (the job took workers down more than max attempts + 2 times).

## 6. Metrics (`GET /metrics`, Prometheus text 0.0.4) and what each alarms on

Counters live in Redis, so any API node shows the cluster's totals. Suggested alerts (thresholds are starting points):

| Metric | Alarm |
|---|---|
| `doc_store_up` | `== 0` for 1 min: Redis unreachable (new requests get 503; work resumes by itself). |
| `doc_queue_depth{state="queued"}`, `doc_queue_oldest_waiting_seconds` | Oldest waiting > 60 s for 5 min: not enough workers, or all stuck. |
| `doc_queue_depth{state="delayed"}` | Growing: jobs keep failing transiently (look at `doc_job_failures_total`). |
| `doc_workers_alive` | `< 1` (or fewer than deployed): no process is reporting. `doc_worker_up{worker=...}` names the missing one. |
| `rate(doc_job_failures_total[5m])` by `code` | `kind="transient"` bursts: an upstream is down (`catalog_unavailable`, `repo_unavailable`) or the docx worker is sick. `kind="permanent"` ones are request problems (`resource_not_found`) unless they spike. |
| `rate(doc_jobs_total{outcome="failed"}[15m])` | Any sustained rate: users are getting FAILURE. |
| `doc_stalled_recoveries_total` | Any increase: a worker died mid-job (OOM, SIGKILL, host loss). |
| `doc_stage_duration_seconds` (p95 by `stage`) | A stage far above its norm (docx: seconds; locate and provision: tens of seconds cold). |
| `doc_repo_clones_total` | A rate much higher than the number of distinct repositories per `ASSET_CACHING_PERIOD`: the clone lock is off or failing (`clone lock unavailable`). |
| `doc_catalog_refresh_failures_total`, `doc_catalog_last_success_timestamp_seconds` | `time() - last_success > 900`: the data API has been unreachable for 15 min. |
| `doc_docx_pool_restarts_total{reason}` | `crash` or `deadline` > 0 repeatedly: the generator is failing; `memory`/`recycle`: expected slowly. |
| `doc_email_total{outcome="failed"\|"in_doubt"}` | SMTP trouble; the documents were delivered regardless. |
| `doc_cache_hits_total{kind}` | Informational (hit ratio). |

`GET /health/status` is DOC's `[{"status":"ok"}, 200]` tuple with more in the first element: `ready` and `problems` (Redis
unreachable, no live worker, disk nearly full), `redis`, `queue` (depth, `lag_s`), `processes` (each with `alive`, its docx pool,
its last catalog fetch), `catalog.last_success_unix_s`, `disk`. `status` stays `ok` while the process answers (liveness); use
`ready` for readiness.

## 7. Symptoms and what to do

| Symptom | Likely cause | Action |
|---|---|---|
| Requests get 503 `queue_unavailable` (with `Retry-After`) | Redis down or unreachable | Restore Redis. Nothing else: the API and workers retry with backoff and resume; polling keeps working after. If Redis lost its data see section 8. |
| 503 `disk_space_low` | Output filesystem below `DOC_JOBD_MIN_FREE_MB` | Free space (the janitor already deletes expired outputs; lower `ASSET_CACHING_PERIOD` or `DOC_JOBD_OUTPUT_GRACE_MS`), or enlarge the volume. Requests resume by themselves. |
| 501 `pdf_not_implemented` | `generate_pdf=true` on `POST /documents` while the API role has no PDF engine configured (`DOC_PDF_ENGINE`) | Set `DOC_PDF_ENGINE=remote` and `DOC_PYWORKER_URL` on **both** the API and the worker (the compose file does); a PDF request is accepted by the API only when it knows an engine exists. |
| Jobs stay `PENDING` | No live worker (see `doc_workers_alive`), or the queue is long, or the id is unknown (Redis lost its data) | Start workers; check `/health/status` `problems`. |
| Many `attempt failed (transient)` with `catalog_unavailable` | The data API is down | Nothing to do; jobs retry with jittered backoff up to `max` attempts over minutes; raise `DOC_JOBD_MAX_ATTEMPTS` for longer outages. Reads are served from the stale catalog for up to 24 h (`serving the stale catalog`). |
| `repo_unavailable` | Git host down, repository missing (`Repository not found` is not retried within a request), or a clone timeout | Check the host; the stale copy is used when there is one. |
| `docx_worker_timeout` / `core_timeout` | A hung process was killed at its deadline | If frequent, look at the document that triggers it (the log line has the key and request id); raise the deadline only for genuinely large books. |
| `stalled recoveries` increasing | Workers dying | Look for OOM kills (`dmesg`), reduce `DOC_JOBD_CONCURRENCY`, lower `DOC_DOCX_RSS_CEILING_MB`. |
| A user says the email never came | `doc_email_total{outcome}` / the document's `findings` | `email_not_sent` (refused for good) or `email_in_doubt` (an attempt died or lost the connection after the message was handed to the server: not resent, by design) are in `<key>.findings.json`. |

### Email, exactly

The email is sent after the document exists, once per **task** (the idempotency key is the task id; a second *request* for the
same document is a second email, a retry or a redelivery of the first is not). The store record goes `none -> sending -> sent`:
claimed before the SMTP conversation, confirmed after the server's final `250`. A refusal before the message was accepted
releases the claim and the job retries the step; no final answer after the message was handed over leaves the claim in place,
reports `email_in_doubt` and never sends again. So a crash can lose an email (reported) but cannot duplicate one.

### Replaying a failed job

A failed job keeps its record for 24 h (`DOC_JOBD_*` `result_ttl`). Its request is in the job hash:

```
scripts/replay.sh <task_id>      # prints the old failure, then POSTs the same body again and prints the new task id
```
(`redis-cli HGET docjobs:job:<task_id> payload` is the request; `HGET ... failure` is why it failed; a failure releases the
in-flight claim, so the re-post is a new job.)

## 8. Recovering from a Redis loss

* **Restart with persistence (AOF everysec):** nothing to do. In the tests (`redis restart mid-run`): the running job finished on
  attempt 1, a queued job survived, the API answered typed 503 for the 1.5 s the server was down.
* **Data lost (no persistence, or the volume is gone):** the queue, job states, locks and counters are gone. Workers notice (lost
  leases, the consumer group is re-created automatically) and keep running; `/health/status` goes `ready: true` again at the
  next beat. Clients that were polling get `PENDING` for ids the new Redis does not know (as Celery does for unknown ids) and the
  UI keeps polling: ask users to resubmit, or restart the UI polling by resubmitting; a resubmission is a new job. Documents already
  in the output directory are cache hits. Verified by `losing_redis_without_persistence_loses_queued_state_but_the_service_heals`.
* **Redis unreachable for a long time:** jobs running at the time finish and keep trying to record their result for about 10 minutes;
  after that the lease lapses and the job is re-run when Redis returns (a second run is a cache hit if the document was placed).

## 9. Running behind the existing reverse proxy (no docker here, so this is a design, not a build)

The service runs beside DOC's FastAPI and replaces nothing. The frontend calls the API directly from the browser through
`PUBLIC_BACKEND_API_URL`; a path-prefix gateway sends the migrated routes to this service:

```
location ~ ^/(documents_docx|documents|task_status|health/status|chapters_in_books|language_codes_and_names|book_codes_for_lang|book_codes_for_lang_from_usfm_only|shared_book_codes|resource_types|findings)(/|$) { proxy_pass http://doc-jobd:8081; }
location / { proxy_pass http://api:5005; }   # everything else (and STET/Passages) stays on FastAPI
```
Rollback is one gateway line. Share the output volume (`shared`) with the file server, keep `<key>.<ext>` naming, and use a
Redis (or Valkey) database or `DOC_JOBD_NAMESPACE` that Celery does not use (Celery uses `redis://redis:6379/0`; this service's keys all
start with `docjobs:` so they coexist). A container would be: the release binary, `python3` with `python/` and `docx_worker.py`,
`git`, and DOC's `backend/templates`; stop grace period >= drain deadline + 30 s; one worker container per CPU pair.
