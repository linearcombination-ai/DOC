/**
 * Operator tool for the job queue (read the RUNBOOK first). It talks to Redis with the same environment variables as the service
 * (REDIS_URL, REDIS_PASSWORD, REDIS_TLS_*, QUEUE_PREFIX, QUEUE_NAME).
 *
 *   node tools/queue-admin.ts status                 counts by state, oldest waiting job, workers with a fresh heartbeat
 *   node tools/queue-admin.ts failed [limit=20]      failed jobs: id, key, request id, error code, attempts, when
 *   node tools/queue-admin.ts show <id>              one job in full (data, progress, failure result, stack)
 *   node tools/queue-admin.ts replay <id>            put a failed job back in the queue (same data, attempts reset); safe: the document is a pure
 *                                                    function of the request key and the email step has its own record
 *   node tools/queue-admin.ts replay-all-failed      replay every failed job
 *   node tools/queue-admin.ts drain-status           what is still running: active jobs and which workers hold them (use before a restart)
 *   node tools/queue-admin.ts forget-email <id>      delete the email record of a job whose outcome is "unknown" so a replay may send it (a human decision)
 */
import { Queue } from "bullmq";
import { loadJobConfig } from "../src/jobs/config.ts";
import { emailRecordKey } from "../src/jobs/email.ts";
import { connectRedis } from "../src/jobs/redis.ts";
import { Telemetry } from "../src/obs/telemetry.ts";

const jobs = loadJobConfig();
const connection = connectRedis(jobs, { maxRetriesPerRequest: 2 });
const queue = new Queue(jobs.QUEUE_NAME, { connection, prefix: jobs.QUEUE_PREFIX });
const [cmd, arg] = process.argv.slice(2);
const when = (ms?: number): string => (ms ? new Date(ms).toISOString() : "-");
try {
  if (cmd === "status") {
    console.log(JSON.stringify(await queue.getJobCounts("waiting", "active", "delayed", "failed", "completed"), null, 2));
    const oldest = (await queue.getWaiting(0, 0))[0];
    console.log(`oldest waiting: ${oldest ? `${Math.round((Date.now() - oldest.timestamp) / 1000)} s (job ${oldest.id})` : "none"}`);
    for (const w of await new Telemetry(connection, jobs.QUEUE_PREFIX).workers()) console.log(`worker ${w.id}: pid ${w.pid}, running ${w.running}, heartbeat ${Math.round((Date.now() - w.at) / 1000)} s ago, rss ${Math.round(w.rssBytes / 1048576)} MB`);
  } else if (cmd === "failed") {
    for (const j of await queue.getFailed(0, Number(arg ?? 20) - 1)) {
      let code = "?";
      try {
        code = (JSON.parse(j.failedReason ?? "{}") as { code?: string }).code ?? j.failedReason ?? "?";
      } catch {
        code = j.failedReason ?? "?";
      }
      console.log(`${j.id}\t${(j.data as { key?: string }).key}\treq ${(j.data as { requestId?: string }).requestId}\t${code}\tattempts ${j.attemptsMade}\tfinished ${when(j.finishedOn)}`);
    }
  } else if (cmd === "show" && arg) {
    const j = await queue.getJob(arg);
    console.log(j ? JSON.stringify({ id: j.id, data: j.data, progress: j.progress, attemptsMade: j.attemptsMade, failedReason: j.failedReason, returnvalue: j.returnvalue, timestamp: when(j.timestamp), processedOn: when(j.processedOn), finishedOn: when(j.finishedOn), stacktrace: j.stacktrace }, null, 2) : `no job ${arg}`);
  } else if (cmd === "replay" && arg) {
    const j = await queue.getJob(arg);
    if (!j) console.log(`no job ${arg}`);
    else {
      await j.retry("failed", { resetAttemptsMade: true });
      console.log(`job ${arg} is back in the queue`);
    }
  } else if (cmd === "replay-all-failed") {
    let n = 0;
    for (const j of await queue.getFailed(0, 999)) {
      await j.retry("failed", { resetAttemptsMade: true });
      n += 1;
    }
    console.log(`${n} failed jobs replayed`);
  } else if (cmd === "drain-status") {
    const active = await queue.getActive(0, 99);
    console.log(`${active.length} active job(s)`);
    for (const j of active) console.log(`${j.id}\t${(j.data as { key?: string }).key}\tstate ${(j.progress as { state?: string }).state ?? "?"}\tstarted ${when(j.processedOn)}`);
    console.log(`${(await queue.getJobCounts("waiting")).waiting ?? 0} waiting; send SIGTERM to a worker to stop it taking more (it finishes its running jobs, up to SHUTDOWN_GRACE_MS)`);
  } else if (cmd === "forget-email" && arg) {
    const j = await queue.getJob(arg);
    const d = j?.data as { submissionId?: string; key?: string; request?: { email_address?: string } } | undefined;
    if (!d?.submissionId || !d.key || !d.request?.email_address) console.log("that job has no email request");
    else console.log(`deleted ${await connection.del(emailRecordKey(jobs.QUEUE_PREFIX, { submissionId: d.submissionId, to: d.request.email_address, documentKey: d.key }))} email record(s)`);
  } else {
    console.log("usage: status | failed [limit] | show <id> | replay <id> | replay-all-failed | drain-status | forget-email <id>");
    process.exitCode = 2;
  }
} finally {
  await queue.close();
  connection.disconnect();
}
