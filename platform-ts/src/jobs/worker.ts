/** The worker side: a BullMQ Worker whose processor runs the pipeline and turns typed errors into retries or permanent failures. */
import { hostname } from "node:os";
import { readdir, rm, stat } from "node:fs/promises";
import { join } from "node:path";
import { UnrecoverableError, Worker, type Job } from "bullmq";
import { Telemetry, type WorkerBeat } from "../obs/telemetry.ts";
import { noMetrics, type Metrics } from "../obs/metrics.ts";
import { classify, JobError } from "./errors.ts";
import { runDocumentJob, type JobContext, type PipelineDeps } from "./pipeline.ts";
import { backoffMs } from "./queue.ts";
import { connectRedis } from "./redis.ts";
import { JobData, type FailureResult, type Stage } from "./types.ts";
import { recoverScratch } from "./scratch.ts";

export interface RunningWorker {
  worker: Worker;
  /** Stop taking jobs and wait for the running ones (graceful drain); `force` abandons them (they become stalled and run elsewhere). */
  close(force?: boolean): Promise<void>;
  /** Jobs this worker has finished (any outcome) and is running now. */
  counts(): { done: number; running: number };
  id: string;
}

export interface WorkerExtras {
  metrics?: Metrics;
  telemetry?: Telemetry;
}

export async function startWorker(deps: PipelineDeps, extras: WorkerExtras = {}): Promise<RunningWorker> {
  const { jobs, log } = deps;
  const metrics = extras.metrics ?? deps.metrics ?? noMetrics;
  const id = `${hostname()}-${process.pid}-${Math.floor(Math.random() * 1e6).toString(36)}`;
  const wlog = log.child({ component: "worker", pid: process.pid, worker: id });
  const removed = await recoverScratch(jobs.WORKING_DIR);
  if (removed.length > 0) wlog.warn({ removed }, "removed scratch directories left by dead workers");
  const connection = connectRedis(jobs, { maxRetriesPerRequest: null });
  connection.on("error", (e: Error) => wlog.warn({ err: e.message }, "redis connection error"));
  let running = 0;
  let done = 0;
  const wdeps: PipelineDeps = { ...deps, metrics };

  const worker = new Worker<JobData, string>(
    jobs.QUEUE_NAME,
    async (job: Job<JobData, string>) => {
      running += 1;
      try {
        return await processJob(wdeps, job, metrics);
      } finally {
        running -= 1;
        done += 1;
      }
    },
    {
      connection,
      prefix: jobs.QUEUE_PREFIX,
      concurrency: jobs.WORKER_CONCURRENCY,
      lockDuration: jobs.LOCK_DURATION_MS,
      stalledInterval: jobs.STALLED_INTERVAL_MS,
      maxStalledCount: jobs.MAX_STALLED_COUNT,
      settings: { backoffStrategy: (attemptsMade) => backoffMs(jobs, attemptsMade) },
    },
  );
  worker.on("error", (e: Error) => wlog.warn({ err: e.message }, "worker error"));
  worker.on("stalled", (jobId) => {
    metrics.inc("doc_stalled_recoveries_total");
    wlog.warn({ job: jobId }, "job stalled: its worker stopped renewing the lock; it will be run again");
  });
  worker.on("completed", (job) => wlog.info({ job: job.id, key: job.data.key, reqId: job.data.requestId, attempts: job.attemptsMade }, "job completed"));
  worker.on("failed", (job, e) => wlog.error({ job: job?.id, key: job?.data.key, reqId: job?.data.requestId, attempts: job?.attemptsMade, err: e.message }, "job failed"));
  await worker.waitUntilReady();

  const beat = (): WorkerBeat => ({
    id,
    pid: process.pid,
    host: hostname(),
    startedAt,
    running,
    rssBytes: process.memoryUsage.rss(),
    docxMode: wdeps.docx.mode,
    pool: (() => {
      const s = wdeps.docx.stats();
      return { size: s.size, idle: s.idle, busy: s.busy, starting: s.starting, recycled: s.recycled, crashed: s.crashed };
    })(),
    jobsDone: done,
  });
  const startedAt = Date.now();
  const timers: NodeJS.Timeout[] = [];
  if (extras.telemetry) {
    const telemetry = extras.telemetry;
    const send = (): void => {
      telemetry.beat(beat(), jobs.HEARTBEAT_INTERVAL_MS * 3).catch((e: unknown) => wlog.debug({ err: e instanceof Error ? e.message : String(e) }, "heartbeat not stored"));
    };
    send();
    timers.push(setInterval(send, jobs.HEARTBEAT_INTERVAL_MS).unref());
  }
  if (jobs.OUTPUT_RETENTION_MINUTES > 0) {
    const sweep = (): void => {
      sweepOutput(jobs.DOCUMENT_OUTPUT_DIR, jobs.OUTPUT_RETENTION_MINUTES * 60_000)
        .then((n) => n > 0 && wlog.info({ removed: n, retentionMinutes: jobs.OUTPUT_RETENTION_MINUTES }, "removed expired documents"))
        .catch((e: unknown) => wlog.warn({ err: e instanceof Error ? e.message : String(e) }, "output sweep failed"));
    };
    sweep();
    timers.push(setInterval(sweep, jobs.OUTPUT_SWEEP_INTERVAL_MS).unref());
  }
  wlog.info({ queue: jobs.QUEUE_NAME, prefix: jobs.QUEUE_PREFIX, concurrency: jobs.WORKER_CONCURRENCY, lockDurationMs: jobs.LOCK_DURATION_MS, docxMode: wdeps.docx.mode }, "worker ready");
  return {
    worker,
    id,
    counts: () => ({ done, running }),
    async close(force = false) {
      for (const t of timers) clearInterval(t);
      await worker.close(force);
      await extras.telemetry?.retire(id).catch(() => undefined);
      connection.disconnect();
    },
  };
}

/** Delete documents (and their findings and partial files) older than the retention window. Returns how many files were removed. */
export async function sweepOutput(dir: string, retentionMs: number, now = Date.now()): Promise<number> {
  let removed = 0;
  let names: string[] = [];
  try {
    names = await readdir(dir);
  } catch {
    return 0;
  }
  for (const name of names) {
    if (!/\.(docx|html|pdf|json|partial|part)$/.test(name)) continue;
    const path = join(dir, name);
    const info = await stat(path).catch(() => undefined);
    if (info?.isFile() && now - info.mtimeMs > retentionMs) {
      await rm(path, { force: true });
      removed += 1;
    }
  }
  return removed;
}

async function processJob(deps: PipelineDeps, job: Job<JobData, string>, metrics: Metrics): Promise<string> {
  const { jobs, log } = deps;
  const attempt = job.attemptsMade + 1;
  const maxAttempts = job.opts.attempts ?? jobs.JOB_ATTEMPTS;
  const parsed = JobData.safeParse(job.data);
  const reqId = parsed.success ? parsed.data.requestId : "";
  const jlog = log.child({ component: "job", job: job.id, attempt, of: maxAttempts, reqId });
  if (!parsed.success) return fail(jlog, metrics, new JobError("invalid_request", `the job data is not a document job: ${parsed.error.message}`, { transient: false }), attempt, maxAttempts);
  const queuedMs = job.processedOn ? job.processedOn - job.timestamp : undefined;
  jlog.info({ key: parsed.data.key, kind: parsed.data.kind, ...(attempt === 1 && queuedMs !== undefined ? { queuedMs } : {}) }, "attempt started");
  const ctx: JobContext = {
    jobId: job.id ?? "unknown",
    attempt,
    maxAttempts,
    signal: AbortSignal.timeout(jobs.JOB_DEADLINE_MS),
    async setState(stage: Stage) {
      await job.updateProgress({ state: stage, attempt });
      jlog.info({ stage, key: parsed.data.key }, "stage");
    },
  };
  const started = performance.now();
  try {
    const outcome = await runDocumentJob(deps, parsed.data, ctx);
    const ms = Math.round(performance.now() - started);
    metrics.inc("doc_job_attempts_total", { result: outcome.cacheHit ? "cache_hit" : "ok" });
    metrics.observe("doc_job_duration_seconds", { result: outcome.cacheHit ? "cache_hit" : "ok" }, ms / 1000);
    jlog.info({ key: outcome.key, ms, cacheHit: outcome.cacheHit, findings: outcome.findings, timings: outcome.timings }, "job finished");
    return outcome.key;
  } catch (error) {
    return fail(jlog, metrics, classify(error), attempt, maxAttempts, Math.round(performance.now() - started));
  }
}

/** One structured line per failed attempt (`code`, `transient`, `retrying`), then either a retry (plain Error) or a permanent failure (UnrecoverableError). */
function fail(jlog: PipelineDeps["log"], metrics: Metrics, error: JobError, attempt: number, maxAttempts: number, ms = 0): never {
  const last = attempt >= maxAttempts;
  const retrying = error.transient && !last;
  jlog.warn({ code: error.code, transient: error.transient, retrying, next_in_ms: retrying ? "backoff" : null, err: error.message, detail: error.detail, ms }, retrying ? "attempt failed, will retry" : "attempt failed, giving up");
  metrics.inc("doc_job_attempts_total", { result: retrying ? "retry" : "failed" });
  metrics.inc("doc_job_failures_total", { code: error.code, retried: String(retrying) });
  metrics.observe("doc_job_duration_seconds", { result: retrying ? "retry" : "failed" }, ms / 1000);
  const result: FailureResult = { code: error.code, message: error.message.slice(0, 500), retryable: error.transient };
  if (!error.transient || last) throw new UnrecoverableError(JSON.stringify(result));
  throw new Error(JSON.stringify(result));
}
