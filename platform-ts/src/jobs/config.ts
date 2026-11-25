import { z } from "zod";
import { ConfigError, parseEnv } from "../util/configError.ts";

const Int = (def: number) => z.coerce.number().int().positive().default(def);
const NonNeg = (def: number) => z.coerce.number().int().nonnegative().default(def);
const Bool = z.enum(["true", "false", "1", "0"]).transform((v) => v === "true" || v === "1");

/** Everything the job runner reads from the environment, validated once at start (API and worker both). */
export const JobEnvSchema = z.object({
  /** `redis://` or `rediss://` (TLS). A password may be in the URL, or in REDIS_PASSWORD (kept out of process listings and logs). */
  REDIS_URL: z.string().default("redis://127.0.0.1:6379/0"),
  REDIS_USERNAME: z.string().default(""),
  REDIS_PASSWORD: z.string().default(""),
  /** Use TLS even when the URL says `redis://`. */
  REDIS_TLS: Bool.default(false),
  REDIS_TLS_CA: z.string().default(""),
  REDIS_TLS_CERT: z.string().default(""),
  REDIS_TLS_KEY: z.string().default(""),
  REDIS_TLS_SERVERNAME: z.string().default(""),
  /** Setting this false turns certificate verification off: a deliberate, logged choice. */
  REDIS_TLS_REJECT_UNAUTHORIZED: Bool.default(true),
  REDIS_CONNECT_TIMEOUT_MS: Int(10_000),

  /** BullMQ key prefix: a separate prefix (or DB) keeps these keys away from Celery's on a shared Redis. */
  QUEUE_PREFIX: z.string().regex(/^[A-Za-z0-9_-]{1,40}$/, "letters, digits, - and _ only, at most 40").default("docp6"),
  QUEUE_NAME: z.string().regex(/^[A-Za-z0-9_-]{1,40}$/, "letters, digits, - and _ only, at most 40").default("documents"),

  /** Where finished documents are placed (DOC's DOCUMENT_OUTPUT_DIR). */
  DOCUMENT_OUTPUT_DIR: z.string().default("document_output"),
  /** Per-job scratch directories live under here (DOC's shared WORKING_DIR is the hazard this replaces). */
  WORKING_DIR: z.string().default("working_temp"),
  /** A finished document younger than this is served as is (DOC's 30 minute mtime TTL, in minutes). */
  OUTPUT_TTL_MINUTES: Int(30),
  /** Finished documents older than this are deleted by the sweeper (0 turns the sweeper off). Must not be below OUTPUT_TTL_MINUTES. */
  OUTPUT_RETENTION_MINUTES: NonNeg(1_440),
  OUTPUT_SWEEP_INTERVAL_MS: Int(600_000),
  /** Below this much free space on the output or working volume a job waits (retries) instead of filling the disk; the API refuses new jobs at half of it. */
  DISK_MIN_FREE_MB: NonNeg(200),

  /** Request limits (API). */
  MAX_BODY_BYTES: Int(262_144),
  /** `off` answers POST /documents with generate_pdf=true with the typed 501; `weasyprint` runs the PDF stage through the Python seam. */
  PDF_ENGINE: z.enum(["weasyprint", "off"]).default("weasyprint"),
  /** `/health/status` reports degraded when the oldest waiting job is older than this. */
  QUEUE_LAG_WARN_MS: Int(60_000),
  /** `/health/status` reports degraded when no catalog fetch succeeded within this many seconds (0: never). */
  CATALOG_STALE_WARN_SECONDS: NonNeg(3_600),
  /** `fixed`-mode ceilings: at most this many resources in one document request, and this many book codes in a path. */
  MAX_RESOURCE_REQUESTS: Int(200),
  MAX_BOOK_CODES: Int(100),

  /** Total attempts per job (first run included). */
  JOB_ATTEMPTS: Int(4),
  JOB_BACKOFF_BASE_MS: Int(2_000),
  JOB_BACKOFF_CAP_MS: Int(60_000),
  /** Hard ceiling for one attempt, all stages together (generous: a cold clone plus a docx). */
  JOB_DEADLINE_MS: Int(900_000),
  STAGE_LOCATE_DEADLINE_MS: Int(120_000),
  STAGE_PROVISION_DEADLINE_MS: Int(600_000),
  STAGE_PARSE_DEADLINE_MS: Int(120_000),
  STAGE_ASSEMBLE_DEADLINE_MS: Int(120_000),
  STAGE_DOCX_DEADLINE_MS: Int(300_000),
  STAGE_EMAIL_DEADLINE_MS: Int(120_000),

  /** BullMQ: a lock not renewed for this long makes the job stalled (a SIGKILLed worker stops renewing). */
  LOCK_DURATION_MS: Int(30_000),
  STALLED_INTERVAL_MS: Int(15_000),
  /** How many times a job may be recovered from stalled before BullMQ fails it. */
  MAX_STALLED_COUNT: Int(3),
  WORKER_CONCURRENCY: Int(2),
  /** Time `worker.close()` waits for in-flight jobs before the process exits anyway (the documented drain deadline). */
  SHUTDOWN_GRACE_MS: Int(120_000),
  /** The API stops accepting, lets in-flight requests finish for this long, then exits. */
  API_DRAIN_MS: Int(10_000),
  /** Completed and failed job records are kept this long (the documents are the durable result). */
  KEEP_JOBS_SECONDS: Int(86_400),

  /** Cross-process single flight for repository clones: lease, and how long a waiter polls before it clones anyway. */
  CLONE_LOCK_LEASE_MS: Int(30_000),
  CLONE_LOCK_POLL_MS: Int(100),
  /** Turn the cross-process lock off (a single worker, or workers that do not share an asset directory). */
  CLONE_LOCK: Bool.default(true),

  PYTHON_BIN: z.string().default("python3"),
  DOCX_WORKER_SCRIPT: z.string().default("python/docx_worker.py"),
  DOCX_SERVER_SCRIPT: z.string().default("python/docx_server.py"),
  /** `process`: one Python process per job. `persistent`: a pool of supervised long-lived processes speaking JSON lines. */
  DOCX_MODE: z.enum(["process", "persistent"]).default("persistent"),
  DOCX_POOL_SIZE: Int(2),
  /** Recycle a persistent worker after this many jobs (a bound on slow leaks). */
  DOCX_RECYCLE_AFTER: Int(200),
  /** A persistent worker whose resident memory passes this is killed (mid-job: the job fails transient) or recycled (idle). */
  DOCX_MEMORY_CEILING_MB: Int(700),
  /** The ceiling for PDF jobs: WeasyPrint on a whole book with notes and words reached 360 MB in the e2e run, a docx 315 MB. */
  PDF_MEMORY_CEILING_MB: Int(2_500),
  DOCX_START_TIMEOUT_MS: Int(20_000),
  /** Test-only: DOCX_WORKER_FAULT is handed to the Python worker (hang, crash, garbage, slow:ms, memhog:mb, crash_once:path, hang_once:path). */
  DOCX_FAULT: z.string().default(""),
  /** Test-only: make the core stage spin forever on its thread (`spin`), or only the first time (`spin_once:<marker path>`). */
  CORE_FAULT: z.string().default(""),
  /** Test-only: `after_email_once:<marker path>` fails the first attempt right after the email step, to prove a retry never sends the email again. */
  JOB_FAULT: z.string().default(""),
  EXPOSE_FAILURE_DETAIL: Bool.default(true),

  /** The email step, with DOC's setting names: sent only when SEND_EMAIL is true and SMTP_HOST is set; otherwise a request that asks for email gets a finding and no email. */
  SEND_EMAIL: Bool.default(false),
  EMAIL_SEND_SUBJECT: z.string().default("Document request"),
  /** `auto`: STARTTLS when the server offers it; `require`: fail without it (DOC's smtplib code always starts TLS); `off`: never (a local test server). */
  SMTP_STARTTLS: z.enum(["auto", "require", "off"]).default("auto"),
  /** Send the email even when the document was already fresh (DOC does not; a user who asks for an email expects one). */
  EMAIL_ON_CACHE_HIT: Bool.default(true),
  SMTP_HOST: z.string().default(""),
  SMTP_PORT: Int(25),
  SMTP_SECURE: Bool.default(false),
  SMTP_USER: z.string().default(""),
  SMTP_PASSWORD: z.string().default(""),
  FROM_EMAIL_ADDRESS: z.string().default("doc@localhost"),
  /** Base URL used in the email body to link the document; empty: attach the file. */
  EMAIL_ATTACH_MAX_MB: Int(20),
  SMTP_TIMEOUT_MS: Int(30_000),
  /** How long the record "this email was sent" is kept (a redelivery within this window never sends again). */
  EMAIL_RECORD_DAYS: Int(14),

  /** The worker's own health listener for container health checks (0: off). */
  WORKER_HEALTH_PORT: z.coerce.number().int().nonnegative().max(65_535).default(0),
  WORKER_HEALTH_HOST: z.string().default("0.0.0.0"),

  /** Heartbeat interval and expiry for worker liveness in Redis. */
  HEARTBEAT_INTERVAL_MS: Int(5_000),
});

export type JobConfig = z.infer<typeof JobEnvSchema>;

export function loadJobConfig(env: NodeJS.ProcessEnv = process.env): JobConfig {
  const jobs = parseEnv(JobEnvSchema, env);
  const problems = crossChecks(jobs);
  if (problems.length > 0) throw new ConfigError(problems);
  return jobs;
}

/** Relations between settings that each pass on their own but cannot work together. */
export function crossChecks(j: JobConfig): string[] {
  const p: string[] = [];
  const stages: [string, number][] = [
    ["STAGE_LOCATE_DEADLINE_MS", j.STAGE_LOCATE_DEADLINE_MS],
    ["STAGE_PROVISION_DEADLINE_MS", j.STAGE_PROVISION_DEADLINE_MS],
    ["STAGE_PARSE_DEADLINE_MS", j.STAGE_PARSE_DEADLINE_MS],
    ["STAGE_ASSEMBLE_DEADLINE_MS", j.STAGE_ASSEMBLE_DEADLINE_MS],
    ["STAGE_DOCX_DEADLINE_MS", j.STAGE_DOCX_DEADLINE_MS],
    ["STAGE_EMAIL_DEADLINE_MS", j.STAGE_EMAIL_DEADLINE_MS],
  ];
  for (const [name, ms] of stages) if (ms > j.JOB_DEADLINE_MS) p.push(`${name}=${ms} is longer than JOB_DEADLINE_MS=${j.JOB_DEADLINE_MS}: the job deadline would always fire first`);
  if (j.JOB_BACKOFF_BASE_MS > j.JOB_BACKOFF_CAP_MS) p.push(`JOB_BACKOFF_BASE_MS=${j.JOB_BACKOFF_BASE_MS} is larger than JOB_BACKOFF_CAP_MS=${j.JOB_BACKOFF_CAP_MS}`);
  if (j.STALLED_INTERVAL_MS > j.LOCK_DURATION_MS) p.push(`STALLED_INTERVAL_MS=${j.STALLED_INTERVAL_MS} is larger than LOCK_DURATION_MS=${j.LOCK_DURATION_MS}: a dead worker would be noticed later than its lock expires`);
  if (j.OUTPUT_RETENTION_MINUTES !== 0 && j.OUTPUT_RETENTION_MINUTES < j.OUTPUT_TTL_MINUTES) p.push(`OUTPUT_RETENTION_MINUTES=${j.OUTPUT_RETENTION_MINUTES} is below OUTPUT_TTL_MINUTES=${j.OUTPUT_TTL_MINUTES}: documents would be deleted while still considered fresh`);
  if (j.CLONE_LOCK_LEASE_MS < 2 * j.CLONE_LOCK_POLL_MS) p.push(`CLONE_LOCK_LEASE_MS=${j.CLONE_LOCK_LEASE_MS} must be at least twice CLONE_LOCK_POLL_MS=${j.CLONE_LOCK_POLL_MS}`);
  if (j.DOCX_MODE === "persistent" && j.DOCX_POOL_SIZE < 1) p.push("DOCX_POOL_SIZE must be at least 1 in persistent mode");
  if (j.REDIS_TLS_CERT !== "" && j.REDIS_TLS_KEY === "") p.push("REDIS_TLS_CERT is set without REDIS_TLS_KEY");
  if (j.REDIS_TLS_KEY !== "" && j.REDIS_TLS_CERT === "") p.push("REDIS_TLS_KEY is set without REDIS_TLS_CERT");
  try {
    const url = new URL(j.REDIS_URL);
    if (url.protocol !== "redis:" && url.protocol !== "rediss:") p.push(`REDIS_URL must start with redis:// or rediss://, got ${url.protocol}//`);
  } catch {
    p.push("REDIS_URL is not a URL (expected redis://[user:password@]host:port/db)");
  }
  if (j.SMTP_HOST === "" && (j.SMTP_USER !== "" || j.SMTP_PASSWORD !== "")) p.push("SMTP_USER or SMTP_PASSWORD is set but SMTP_HOST is empty");
  if (j.SEND_EMAIL && j.SMTP_HOST === "") p.push("SEND_EMAIL is true but SMTP_HOST is empty");
  return p;
}
