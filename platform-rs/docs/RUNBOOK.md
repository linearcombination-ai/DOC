# Runbook: `doc-jobd` (Rust shell, P6)

For whoever is on call. Everything here was exercised by the test suite or the commands in `README.md` unless it says
**not exercised**. Paths are relative to `p6rs/`; `redis-cli` can be `valkey-cli` (same protocol).

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
| `DOC_DOCX_MODE` | `per_job` | `per_job` (a Python process per document) or `persistent` (a pool, `DOC_DOCX_POOL_SIZE` = concurrency). |
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
`resource_not_found`, `book_not_found`, `invalid_request`, `docx_input_rejected`, `core_rejected`, `resource_unreadable`,
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
| 501 `pdf_not_implemented` | `generate_pdf=true` on `POST /documents` | By design in this build: PDF is not implemented; send `generate_pdf=false` for HTML. |
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
