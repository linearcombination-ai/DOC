/** The API side of the queue: enqueue with de-duplication, status in DOC's legacy shape, queue health. No pipeline code here. */
import { createHash } from "node:crypto";
import type { Redis } from "ioredis";
import { Queue } from "bullmq";
import { connectRedis } from "./redis.ts";
import type { JobConfig } from "./config.ts";
import { JobError } from "./errors.ts";
import { FailureResult, Progress, type JobData, type TaskStatus } from "./types.ts";
import type { Log } from "../util/log.ts";

export const JOB_NAME = "document";

/**
 * What makes two submissions "the same job". The document key alone is not enough: it ignores the recipient and the PDF flag, so
 * two people asking for the same document by email (or one with and one without a PDF) would share one job and one of them would
 * get neither the email nor the PDF. Found while writing the email tests; DOC's Celery path has no de-duplication to compare with.
 */
export function dedupId(data: Pick<JobData, "kind" | "key" | "request">): string {
  const email = typeof data.request["email_address"] === "string" ? data.request["email_address"].toLowerCase() : "";
  const emailPart = email === "" ? "" : createHash("sha256").update(email).digest("hex").slice(0, 12);
  return `${data.kind}:${data.key}:pdf=${data.request["generate_pdf"] === true ? 1 : 0}:${emailPart}`;
}

export interface QueueHealth {
  redis: "up" | "down";
  waiting?: number;
  active?: number;
  delayed?: number;
  failed?: number;
  workers?: number;
  oldestWaitingMs?: number | null;
  error?: string;
}

export function jobOptions(jobs: JobConfig) {
  return {
    attempts: jobs.JOB_ATTEMPTS,
    backoff: { type: "custom" as const },
    removeOnComplete: { age: jobs.KEEP_JOBS_SECONDS },
    removeOnFail: { age: jobs.KEEP_JOBS_SECONDS },
  };
}

/** Full jitter exponential backoff: a delay drawn uniformly from [base, min(cap, base * 2^(attempt-1))] (never zero: a retry storm is worse than a wait). */
export function backoffMs(jobs: JobConfig, attemptsMade: number, random: () => number = Math.random): number {
  const ceiling = Math.min(jobs.JOB_BACKOFF_CAP_MS, jobs.JOB_BACKOFF_BASE_MS * 2 ** Math.max(0, attemptsMade - 1));
  return Math.max(1, Math.floor(jobs.JOB_BACKOFF_BASE_MS / 2 + random() * (ceiling - jobs.JOB_BACKOFF_BASE_MS / 2)));
}

/** What the status path needs from a queue (a real BullMQ Queue, or a fake that simulates a race in the test). */
export interface StatusQueue {
  getJobState(id: string): Promise<string>;
  getJob(id: string): Promise<{ progress: unknown; returnvalue: unknown; failedReason?: string | undefined } | undefined>;
}

/**
 * Celery's `{state, result}` shape from a job. An unknown id is PENDING, as in Celery (documented quirk).
 *
 * The state is read first and the job record second, and a terminal state re-reads the record: reading the record first
 * (the first version) let a poll that landed between "job fetched while active" and "state read after it completed"
 * answer SUCCESS with a null result (the UI then waits forever for `results[1]`) or FAILURE with no reason. Found by
 * running the fault suite under CPU load; `job.status.test.ts` pins it with a queue that simulates the race.
 */
export async function taskStatusFrom(queue: StatusQueue, id: string): Promise<TaskStatus> {
  const state = await queue.getJobState(id);
  if (state === "unknown") return { state: "PENDING" };
  if (state === "completed" || state === "failed") {
    const done = await queue.getJob(id);
    if (!done) return { state: "PENDING" };
    if (state === "completed") return { state: "SUCCESS", result: String(done.returnvalue) };
    return { state: "FAILURE", result: parseFailure(done.failedReason) };
  }
  const job = await queue.getJob(id);
  const progress = Progress.safeParse(job?.progress);
  if (progress.success) return progress.data.attempt > 1 ? { state: progress.data.state, attempt: progress.data.attempt } : { state: progress.data.state };
  return { state: "PENDING" };
}

export class JobService {
  readonly #queue: Queue;
  readonly #connection: Redis;
  readonly #jobs: JobConfig;
  readonly #log: Log;

  constructor(jobs: JobConfig, log: Log) {
    this.#jobs = jobs;
    this.#log = log.child({ component: "queue" });
    // Fail fast when Redis is down: an API request gets a typed 503 now, not a hang until Redis returns.
    this.#connection = connectRedis(jobs, { enableOfflineQueue: false, maxRetriesPerRequest: 1, retryStrategy: (n) => Math.min(n * 200, 2_000), lazyConnect: false });
    this.#connection.on("error", (e: Error) => this.#log.warn({ err: e.message }, "redis connection error"));
    this.#queue = new Queue(jobs.QUEUE_NAME, { connection: this.#connection, prefix: jobs.QUEUE_PREFIX });
    this.#queue.on("error", (e: Error) => this.#log.warn({ err: e.message }, "queue error"));
  }

  /** Enqueue, or join the identical request already in flight. Returns the task id the client polls. */
  async enqueue(data: JobData): Promise<{ id: string; deduplicated: boolean }> {
    try {
      const job = await this.#queue.add(JOB_NAME, data, { ...jobOptions(this.#jobs), deduplication: { id: dedupId(data) } });
      const id = job.id ?? "";
      // BullMQ answers a duplicate with the existing job's id; read it back to see whose submission it carries.
      const stored = await this.#queue.getJob(id);
      const deduplicated = stored !== undefined && stored.data.submissionId !== data.submissionId;
      this.#log.info({ job: id, key: data.key, deduplicated }, deduplicated ? "joined an identical in-flight job" : "job enqueued");
      return { id, deduplicated };
    } catch (error) {
      throw new JobError("internal", `the job queue is unavailable: ${error instanceof Error ? error.message : String(error)}`, { transient: true, cause: error });
    }
  }

  /** `GET /task_status/{id}`: see `taskStatusFrom`. */
  async status(id: string): Promise<TaskStatus> {
    try {
      return await taskStatusFrom(this.#queue, id);
    } catch (error) {
      throw new JobError("internal", `the job queue is unavailable: ${error instanceof Error ? error.message : String(error)}`, { transient: true, cause: error });
    }
  }

  async health(): Promise<QueueHealth> {
    try {
      const [counts, workers, waiting] = await Promise.all([this.#queue.getJobCounts("waiting", "active", "delayed", "failed"), this.#queue.getWorkers(), this.#queue.getWaiting(0, 0)]);
      const oldest = waiting[0]?.timestamp;
      return { redis: "up", waiting: counts["waiting"] ?? 0, active: counts["active"] ?? 0, delayed: counts["delayed"] ?? 0, failed: counts["failed"] ?? 0, workers: workers.length, oldestWaitingMs: oldest === undefined ? null : Date.now() - oldest };
    } catch (error) {
      return { redis: "down", error: error instanceof Error ? error.message : String(error) };
    }
  }

  get queue(): Queue {
    return this.#queue;
  }

  /** The connection the API uses for everything that is not the queue (metrics, heartbeats, the email record): fails fast when Redis is down. */
  get connection(): Redis {
    return this.#connection;
  }

  async close(): Promise<void> {
    await this.#queue.close().catch(() => undefined);
    this.#connection.disconnect();
  }
}

/** The worker stores the failure as JSON in the failure reason; anything else (BullMQ's own "job stalled more than allowable limit") is mapped. */
export function parseFailure(reason: string | undefined): FailureResult {
  if (reason) {
    try {
      const parsed = FailureResult.safeParse(JSON.parse(reason));
      if (parsed.success) return parsed.data;
    } catch {
      // not ours
    }
  }
  if (reason?.includes("stalled")) return { code: "job_stalled", message: "the worker handling the request stopped responding repeatedly", retryable: true };
  return { code: "internal", message: reason ?? "the job failed", retryable: true };
}
