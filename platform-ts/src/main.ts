import { serve } from "@hono/node-server";
import { existsSync } from "node:fs";
import { mkdir, access, constants } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { Acquirer } from "./acquirer/acquirer.ts";
import { createApp, type JobDeps } from "./app.ts";
import { CatalogClient, type CatalogClientOptions } from "./catalog/client.ts";
import { loadConfig, type Config } from "./config.ts";
import { createCore, loadWasm } from "./core/wasm.ts";
import { Lookup } from "./service/lookup.ts";
import { loadJobConfig, type JobConfig } from "./jobs/config.ts";
import { CoreThread } from "./jobs/corethread.ts";
import { ProcessDocxRunner, type DocxRunner } from "./jobs/docx.ts";
import { DocxPool } from "./jobs/docxpool.ts";
import { JobCore } from "./jobs/jobcore.ts";
import { JobService } from "./jobs/queue.ts";
import { checkDisk } from "./jobs/pipeline.ts";
import { describeRedis } from "./jobs/redis.ts";
import { RepoLock } from "./jobs/repolock.ts";
import { RedisMetrics, noMetrics, type Metrics } from "./obs/metrics.ts";
import { Telemetry } from "./obs/telemetry.ts";
import { ConfigError, exitOnConfigError } from "./util/configError.ts";
import { createLogger, type Log } from "./util/log.ts";

const here = dirname(fileURLToPath(import.meta.url));

export type Role = "api" | "worker" | "full";

export function catalogOptions(config: Config, log: Log, profile: "as_built" | "fixed", extra: Pick<CatalogClientOptions, "metrics" | "onFetched"> = {}): CatalogClientOptions {
  return {
    url: config.DATA_API_URL,
    userAgent: config.USER_AGENT_STR,
    requestedWith: config.X_REQUESTED_WITH_VALUE,
    ttlMs: config.DATA_API_CACHE_TTL_SECONDS * 1000,
    timeoutMs: config.DATA_API_TIMEOUT_MS,
    maxAttempts: config.DATA_API_MAX_ATTEMPTS,
    backoff: { baseMs: config.DATA_API_BACKOFF_BASE_MS, capMs: config.DATA_API_BACKOFF_CAP_MS },
    deadlineMs: config.DATA_API_DEADLINE_MS,
    profile,
    log,
    ...extra,
  };
}

/**
 * Wire one process. Roles differ only in which wasm module they load and what they start:
 *  - `api`:    the light module (`core/pkg-api`: request keys, catalog, book lists), no document thread work
 *  - `worker`: the full module (`core/pkg`) on the main thread for lookups and on a thread for parse and assemble
 *  - `full`:   both in one process (tests, a single-process dev run); uses the full module
 * Catalog normalisation runs on the module's thread in every role, so no event loop stalls for 300 ms when the catalog refreshes.
 */
export async function buildApp(config: Config, log: Log, pkgDir: string | undefined = undefined, withJobs: JobConfig | undefined = undefined, role: Role = "full") {
  const fullDir = pkgDir ?? (config.WASM_PKG_DIR || resolve(here, "../core/pkg"));
  const apiDir = config.WASM_API_PKG_DIR || resolve(here, "../core/pkg-api");
  const dir = role === "api" && existsSync(apiDir) ? apiDir : fullDir;
  const thread = new CoreThread(dir, withJobs?.CORE_FAULT ?? "");
  const wasm = loadWasm(dir);
  const core = createCore(wasm, log, thread);
  const jobCore = new JobCore(wasm, thread);

  // Redis-backed telemetry, only when there is a queue
  let service: JobService | undefined;
  let metrics: Metrics = noMetrics;
  let telemetry: Telemetry | undefined;
  let lock: RepoLock | undefined;
  if (withJobs) {
    service = new JobService(withJobs, log);
    metrics = new RedisMetrics(service.connection, withJobs.QUEUE_PREFIX);
    telemetry = new Telemetry(service.connection, withJobs.QUEUE_PREFIX);
    if (withJobs.CLONE_LOCK && role !== "api") lock = new RepoLock({ redis: service.connection, prefix: withJobs.QUEUE_PREFIX, scope: resolve(config.RESOURCE_ASSETS_DIR), leaseMs: withJobs.CLONE_LOCK_LEASE_MS, pollMs: withJobs.CLONE_LOCK_POLL_MS, log, metrics });
  }
  const acquirer = new Acquirer({
    root: resolve(config.RESOURCE_ASSETS_DIR),
    gitBin: config.GIT_BIN,
    userAgent: config.USER_AGENT_STR,
    requestedWith: config.X_REQUESTED_WITH_VALUE,
    stalenessMs: config.ASSET_CACHING_PERIOD * 60_000,
    cloneTimeoutMs: config.GIT_CLONE_TIMEOUT_MS,
    maxAttempts: config.GIT_MAX_ATTEMPTS,
    backoff: { baseMs: config.GIT_BACKOFF_BASE_MS, capMs: config.GIT_BACKOFF_CAP_MS },
    concurrency: config.GIT_CLONE_CONCURRENCY,
    gcGraceMs: config.REQUEST_DEADLINE_MS * 2,
    log,
    metrics,
    ...(lock ? { lock } : {}),
  });
  await acquirer.recover();
  const onFetched = telemetry ? (kind: string): void => void telemetry?.catalogOk(kind).catch(() => undefined) : undefined;
  const extra = { metrics, ...(onFetched ? { onFetched } : {}) };
  const lookup = new Lookup({
    core,
    catalog: { as_built: new CatalogClient(catalogOptions(config, log, "as_built", extra)), fixed: new CatalogClient(catalogOptions(config, log, "fixed", extra)) },
    acquirer,
    config,
    log,
  });
  log.info({ kitchen: core.kitchenVersion, mode: config.DOC_MODE, role, wasm: dir, flags: lookup.flagsFor(config.DOC_MODE) }, "core loaded");
  const docx: DocxRunner | undefined = withJobs && role !== "api" ? buildDocxRunner(withJobs, log, metrics) : undefined;
  const jobs: JobDeps | undefined = withJobs && service && telemetry ? { service, core: jobCore, config: withJobs, metrics, telemetry } : undefined;
  return { app: createApp({ lookup, config, log, metrics, ...(jobs ? { jobs } : {}) }), lookup, acquirer, core, config, jobCore, thread, jobs, docx, metrics, telemetry, role };
}

export function buildDocxRunner(jobs: JobConfig, log: Log, metrics: Metrics): DocxRunner {
  if (jobs.DOCX_MODE === "persistent") {
    return new DocxPool({ pythonBin: jobs.PYTHON_BIN, script: jobs.DOCX_SERVER_SCRIPT, size: jobs.DOCX_POOL_SIZE, recycleAfter: jobs.DOCX_RECYCLE_AFTER, memoryCeilingMb: jobs.DOCX_MEMORY_CEILING_MB, startTimeoutMs: jobs.DOCX_START_TIMEOUT_MS, fault: jobs.DOCX_FAULT, log, metrics });
  }
  return new ProcessDocxRunner({ pythonBin: jobs.PYTHON_BIN, script: jobs.DOCX_WORKER_SCRIPT, fault: jobs.DOCX_FAULT, log, metrics });
}

const run = promisify(execFile);

/**
 * Checks that need the file system, executables and the network, run once before the process takes traffic. Every problem is
 * collected, so the operator fixes them in one pass; `ConfigError` ends the process with exit code 78 and the list.
 */
export async function preflight(config: Config, jobs: JobConfig | undefined, role: Role, wasmDir?: string): Promise<{ warnings: string[] }> {
  const problems: string[] = [];
  const warnings: string[] = [];
  const dirs = jobs ? [jobs.DOCUMENT_OUTPUT_DIR, jobs.WORKING_DIR, config.RESOURCE_ASSETS_DIR] : [config.RESOURCE_ASSETS_DIR];
  for (const dir of dirs) {
    try {
      await mkdir(dir, { recursive: true });
      await access(dir, constants.W_OK);
    } catch (error) {
      problems.push(`${dir} cannot be created or written: ${error instanceof Error ? error.message : String(error)}`);
    }
  }
  const wasm = wasmDir ?? (role === "api" && existsSync(config.WASM_API_PKG_DIR || resolve(here, "../core/pkg-api")) ? config.WASM_API_PKG_DIR || resolve(here, "../core/pkg-api") : config.WASM_PKG_DIR || resolve(here, "../core/pkg"));
  if (!existsSync(`${wasm}/doc_wasm.js`)) problems.push(`the wasm core is not built at ${wasm} (run scripts/build-wasm.sh)`);
  if (jobs && role !== "api") {
    try {
      await run(jobs.PYTHON_BIN, ["-c", "import sys; assert sys.version_info >= (3, 12)"], { timeout: 10_000 });
    } catch {
      problems.push(`PYTHON_BIN=${jobs.PYTHON_BIN} does not run Python 3.12 or newer`);
    }
    for (const [name, path] of [["DOCX_WORKER_SCRIPT", jobs.DOCX_WORKER_SCRIPT], ["DOCX_SERVER_SCRIPT", jobs.DOCX_SERVER_SCRIPT]] as const) if (!existsSync(path)) problems.push(`${name}=${path} does not exist`);
    for (const f of [jobs.REDIS_TLS_CA, jobs.REDIS_TLS_CERT, jobs.REDIS_TLS_KEY]) if (f !== "" && !existsSync(f)) problems.push(`the TLS file ${f} does not exist`);
    if (!jobs.REDIS_TLS_REJECT_UNAUTHORIZED) warnings.push("REDIS_TLS_REJECT_UNAUTHORIZED is false: the Redis server certificate is not verified");
    try {
      await checkDisk(jobs);
    } catch (error) {
      warnings.push(`${error instanceof Error ? error.message : String(error)}`);
    }
    try {
      await run(config.GIT_BIN, ["--version"], { timeout: 10_000 });
    } catch {
      problems.push(`GIT_BIN=${config.GIT_BIN} does not run`);
    }
  }
  if (problems.length > 0) throw new ConfigError(problems);
  return { warnings };
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const config = loadConfig();
    const log = createLogger(config.LOG_LEVEL);
    const jobConfig = process.env["REDIS_URL"] ? loadJobConfig() : undefined;
    const { warnings } = await preflight(config, jobConfig, "api");
    for (const w of warnings) log.warn(w);
    if (jobConfig) log.info({ redis: describeRedis(jobConfig), queue: jobConfig.QUEUE_NAME, prefix: jobConfig.QUEUE_PREFIX, drainMs: jobConfig.API_DRAIN_MS }, "job queue configured");
    const { app, jobs, thread } = await buildApp(config, log, undefined, jobConfig, "api");
    const server = serve({ fetch: app.fetch, port: config.PORT, hostname: config.HOST }, (info) => log.info({ port: info.port, host: config.HOST }, "listening"));
    let stopping = false;
    const stop = (signal: string): void => {
      if (stopping) return;
      stopping = true;
      const drainMs = jobConfig?.API_DRAIN_MS ?? 10_000;
      log.info({ signal, drainMs }, "shutting down: no new connections, finishing in-flight requests");
      const forced = setTimeout(() => {
        log.error({ drainMs }, "drain deadline passed: exiting with requests still open");
        process.exit(1);
      }, drainMs);
      forced.unref();
      server.close(() => {
        void Promise.all([jobs?.service.close(), thread.close()]).finally(() => {
          log.info("drained; bye");
          process.exit(0);
        });
      });
      (server as unknown as { closeIdleConnections?: () => void }).closeIdleConnections?.();
    };
    process.on("SIGTERM", () => stop("SIGTERM"));
    process.on("SIGINT", () => stop("SIGINT"));
  } catch (error) {
    exitOnConfigError(error);
  }
}
