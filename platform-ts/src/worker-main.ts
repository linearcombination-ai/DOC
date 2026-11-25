/** The worker process: `node src/worker-main.ts`. One process, N concurrent jobs, graceful drain on SIGTERM/SIGINT with a deadline. */
import { loadConfig } from "./config.ts";
import { loadJobConfig } from "./jobs/config.ts";
import { describeRedis } from "./jobs/redis.ts";
import { startHealthServer, type WorkerHealth } from "./jobs/health.ts";
import { startWorker } from "./jobs/worker.ts";
import { buildApp, preflight } from "./main.ts";
import { exitOnConfigError } from "./util/configError.ts";
import { createLogger } from "./util/log.ts";

try {
  const config = loadConfig();
  const jobs = loadJobConfig();
  const log = createLogger(config.LOG_LEVEL);
  const { warnings } = await preflight(config, jobs, "worker");
  for (const w of warnings) log.warn(w);
  log.info({ redis: describeRedis(jobs), queue: jobs.QUEUE_NAME, prefix: jobs.QUEUE_PREFIX, docxMode: jobs.DOCX_MODE, concurrency: jobs.WORKER_CONCURRENCY, drainMs: jobs.SHUTDOWN_GRACE_MS }, "worker configured");
  const built = await buildApp(config, log, undefined, jobs, "worker");
  const docx = built.docx;
  if (!docx) throw new Error("the worker has no docx runner");
  await docx.warm();
  const running = await startWorker({ config, jobs, log, lookup: built.lookup, acquirer: built.acquirer, core: built.jobCore, docx, metrics: built.metrics, ...(built.jobs ? { redis: built.jobs.service.connection } : {}) }, { metrics: built.metrics, ...(built.telemetry ? { telemetry: built.telemetry } : {}) });

  const health: WorkerHealth = { accepting: true, id: running.id, running: () => running.counts().running, done: () => running.counts().done, startedAt: Date.now() };
  const healthServer = jobs.WORKER_HEALTH_PORT > 0 ? startHealthServer(jobs.WORKER_HEALTH_PORT, jobs.WORKER_HEALTH_HOST, health) : undefined;
  if (healthServer) log.info({ port: jobs.WORKER_HEALTH_PORT }, "worker health listener");

  let closing = false;
  async function shutdown(signal: string): Promise<void> {
    if (closing) return;
    closing = true;
    health.accepting = false;
    log.info({ signal, graceMs: jobs.SHUTDOWN_GRACE_MS, running: running.counts().running }, "shutting down: no new jobs, draining the running ones");
    const forced = setTimeout(() => {
      log.error({ graceMs: jobs.SHUTDOWN_GRACE_MS }, "drain deadline passed: abandoning running jobs (they will be recovered as stalled by another worker)");
      process.exit(1);
    }, jobs.SHUTDOWN_GRACE_MS);
    forced.unref();
    await running.close();
    await docx?.close();
    await built.jobs?.service.close();
    await built.thread.close();
    healthServer?.close();
    log.info({ done: running.counts().done }, "drained; bye");
    process.exit(0);
  }
  process.on("SIGTERM", () => void shutdown("SIGTERM"));
  process.on("SIGINT", () => void shutdown("SIGINT"));
} catch (error) {
  exitOnConfigError(error);
}
