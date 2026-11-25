/**
 * Fault injection that needs real processes: a worker killed with SIGKILL mid-job, a worker told to stop (SIGTERM) mid-job,
 * and Redis killed or stopped mid-run. Everything is a real process: node workers, the Python docx worker, redis-server.
 */
import { execFileSync, type ChildProcess } from "node:child_process";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { createStack, judeRequest, startRedis, type Stack } from "./support/stack.ts";
import { documentPath } from "../src/jobs/pipeline.ts";
import { pidAlive } from "../src/jobs/docx.ts";

let s: Stack | undefined;
const redisOwned: Awaited<ReturnType<typeof startRedis>>[] = [];
afterEach(async () => {
  await s?.close();
  s = undefined;
  for (const r of redisOwned.splice(0)) {
    await r.kill();
    execFileSync("rm", ["-rf", r.dir]);
  }
});

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const pgrep = (pattern: string): number[] => {
  try {
    return execFileSync("pgrep", ["-f", pattern], { encoding: "utf8" }).split("\n").filter(Boolean).map(Number).filter((p) => p !== process.pid);
  } catch {
    return [];
  }
};
async function until(what: string, test: () => Promise<boolean> | boolean, timeoutMs = 30_000): Promise<void> {
  const end = Date.now() + timeoutMs;
  while (Date.now() < end) {
    if (await test()) return;
    await sleep(25);
  }
  throw new Error(`timed out waiting for ${what}`);
}
const workers = async (st: Stack): Promise<number> => (await st.app.jobs?.service.health())?.workers ?? 0;
const exited = (c: ChildProcess): Promise<number | null> => new Promise((r) => (c.exitCode !== null ? r(c.exitCode) : c.once("exit", (code) => r(code))));
const isZip = (path: string): boolean => readFileSync(path).subarray(0, 2).toString() === "PK";

describe("a worker that dies", () => {
  it("SIGKILL mid-job: the stalled job is recovered by another worker, completes once, and the output is valid; the orphaned Python worker exits by itself", async () => {
    s = await createStack({ env: { LOCK_DURATION_MS: "1500", STALLED_INTERVAL_MS: "500", MAX_STALLED_COUNT: "3" } });
    const a = s.spawnWorker({ DOCX_FAULT: "slow:4000" });
    await until("worker A ready", async () => (await workers(s!)) >= 1);
    const id = String((await s.post()).json["task_id"]);
    await until("conversion started", async () => (await s!.status(id))["state"] === "Converting to Docx");
    const python = pgrep(`docx_worker.py ${s.dirs.work}`);
    expect(python.length).toBe(1);
    a.kill("SIGKILL");
    const killedAt = Date.now();
    await exited(a);
    // the orphan watchdog: the Python process notices its parent is gone and exits (it would otherwise run to completion or hang forever)
    await until("orphaned python gone", () => !pidAlive(python[0] as number), 5_000);
    process.stderr.write(`TIMING sigkill: orphaned python exited ${Date.now() - killedAt} ms after its parent was killed\n`);
    expect(readdirSync(join(s.dirs.work, "jobs")).length).toBe(1); // A's scratch directory is still there: nobody cleaned up after a SIGKILL
    s.spawnWorker();
    const { last } = await s.waitFor(id, ["SUCCESS", "FAILURE"], 60_000);
    expect(last["state"]).toBe("SUCCESS");
    process.stderr.write(`TIMING sigkill: job completed ${Date.now() - killedAt} ms after the SIGKILL (lock ${s.jobConfig.LOCK_DURATION_MS} ms, stalled check every ${s.jobConfig.STALLED_INTERVAL_MS} ms, second worker started after the python exit)`);
    await sleep(200);
    const file = documentPath(s.dirs.out, String(last["result"]));
    expect(isZip(file)).toBe(true);
    const counts = await s.app.jobs?.service.queue.getJobCounts("completed", "failed", "active", "waiting");
    expect(counts).toMatchObject({ completed: 1, failed: 0, active: 0, waiting: 0 });
    // the job ran its stages twice (once in each worker), and finished once
    const seen = s.states.get(id) ?? [];
    expect(seen.filter((x) => x === "Converting to Docx").length).toBe(2);
    expect(readdirSync(join(s.dirs.work, "jobs"))).toEqual([]); // worker B removed the dead worker's scratch directory at start-up
    expect(readdirSync(s.dirs.out).sort()).toEqual([`${String(last["result"])}.docx`, `${String(last["result"])}.findings.json`]);
  }, 120_000);

  it("SIGTERM mid-job: the worker takes no new job, finishes the one it has, and exits 0", async () => {
    s = await createStack();
    const a = s.spawnWorker({ DOCX_FAULT: "slow:2500", WORKER_CONCURRENCY: "1" });
    await until("worker ready", async () => (await workers(s!)) >= 1);
    const first = String((await s.post()).json["task_id"]);
    await until("conversion started", async () => (await s!.status(first))["state"] === "Converting to Docx");
    const termAt = Date.now();
    a.kill("SIGTERM");
    const second = String((await s.post(judeRequest({ use_chapter_labels: true }))).json["task_id"]);
    const code = await exited(a);
    const drainedMs = Date.now() - termAt;
    expect(code).toBe(0);
    expect(drainedMs).toBeGreaterThan(500); // it waited for the running job
    process.stderr.write(`TIMING sigterm: worker exited 0 after ${drainedMs} ms, having finished the running job`);
    const one = await s.status(first);
    expect(one["state"]).toBe("SUCCESS");
    expect(isZip(documentPath(s.dirs.out, String(one["result"])))).toBe(true);
    // the job queued after SIGTERM was not taken by the draining worker
    expect(["PENDING"]).toContain(String((await s.status(second))["state"]));
    expect(pgrep(`docx_worker.py ${s.dirs.work}`)).toEqual([]);
    s.spawnWorker();
    const done = await s.waitFor(second, ["SUCCESS", "FAILURE"], 60_000);
    expect(done.last["state"]).toBe("SUCCESS");
  }, 120_000);
});

describe("a job that keeps killing its workers", () => {
  it("is failed with job_stalled after maxStalledCount recoveries instead of looping forever", async () => {
    s = await createStack({ env: { LOCK_DURATION_MS: "1500", STALLED_INTERVAL_MS: "500", MAX_STALLED_COUNT: "1" } });
    const id = String((await s.post()).json["task_id"]);
    for (let round = 1; round <= 2; round += 1) {
      const w = s.spawnWorker({ DOCX_FAULT: "slow:6000" });
      await until(`worker ${round} converting`, async () => (await s!.status(id))["state"] === "Converting to Docx" && (s!.states.get(id)?.filter((x) => x === "Converting to Docx").length ?? 0) >= round, 40_000);
      w.kill("SIGKILL");
      await exited(w);
    }
    const survivor = s.spawnWorker();
    void survivor;
    const { last } = await s.waitFor(id, ["SUCCESS", "FAILURE"], 60_000);
    expect(last["state"]).toBe("FAILURE");
    expect((last["result"] as { code: string; retryable: boolean })).toMatchObject({ code: "job_stalled", retryable: true });
    expect(readdirSync(s.dirs.out)).toEqual([]);
  }, 150_000);
});

describe("two worker processes on a cold cache", () => {
  it("both finish and every document is valid; every repository is cloned once in total, not once per process (the P5 version of this test allowed 4)", async () => {
    s = await createStack();
    s.spawnWorker();
    s.spawnWorker();
    await until("two workers", async () => (await workers(s!)) >= 2);
    const a = String((await s.post()).json["task_id"]);
    const b = String((await s.post(judeRequest({ use_chapter_labels: true }))).json["task_id"]);
    const [ra, rb] = await Promise.all([s.waitFor(a, ["SUCCESS", "FAILURE"], 60_000), s.waitFor(b, ["SUCCESS", "FAILURE"], 60_000)]);
    expect([ra.last["state"], rb.last["state"]]).toEqual(["SUCCESS", "SUCCESS"]);
    for (const r of [ra, rb]) expect(isZip(documentPath(s.dirs.out, String(r.last["result"])))).toBe(true);
    const clones = s.flaky.cloneCalls();
    process.stderr.write(`TIMING two workers, cold: ${clones} git clone calls for 2 repositories (2 means one clone each)\n`);
    expect(clones).toBe(2);
  }, 120_000);
});

describe("Redis goes away mid-run", () => {
  it("Redis stopped and restarted (clean shutdown) while a job converts: the API answers a typed 503 meanwhile, the worker reconnects, the job completes once", async () => {
    const redis = await startRedis({ aof: true });
    redisOwned.push(redis);
    s = await createStack({ redis, env: { DOCX_FAULT: "slow:2500" } });
    await s.startWorker();
    const id = String((await s.post()).json["task_id"]);
    await until("conversion started", async () => (await s!.status(id))["state"] === "Converting to Docx");
    await redis.stop();
    const during = await s.post(judeRequest({ use_chapter_labels: true }));
    expect(during.status).toBe(503);
    expect(during.json).toMatchObject({ code: "queue_unavailable", retryable: true });
    const health = await (await s.app.app.request("/health/status")).json();
    expect(health).toMatchObject([{ status: "degraded", queue: { redis: "down" } }, 200]);
    await sleep(800);
    const restartedAt = Date.now();
    await redis.restart();
    const { last } = await s.waitFor(id, ["SUCCESS", "FAILURE"], 60_000);
    process.stderr.write(`TIMING redis stop/restart: job SUCCESS ${Date.now() - restartedAt} ms after Redis came back (job needed 2.5 s of conversion)`);
    expect(last["state"]).toBe("SUCCESS");
    expect(isZip(documentPath(s.dirs.out, String(last["result"])))).toBe(true);
    const counts = await s.app.jobs?.service.queue.getJobCounts("completed", "failed");
    expect(counts).toMatchObject({ completed: 1, failed: 0 });
    const after = await (await s.app.app.request("/health/status")).json();
    expect(after).toMatchObject([{ status: "ok", queue: { redis: "up", workers: 1 } }, 200]);
  }, 120_000);

  it("Redis killed (SIGKILL, AOF replayed) mid-run: no document is lost; whatever Redis forgot is redone and the output is valid", async () => {
    const redis = await startRedis({ aof: true });
    redisOwned.push(redis);
    s = await createStack({ redis, env: { DOCX_FAULT: "slow:2500", LOCK_DURATION_MS: "1500", STALLED_INTERVAL_MS: "500" } });
    await s.startWorker();
    const id = String((await s.post()).json["task_id"]);
    await until("conversion started", async () => (await s!.status(id))["state"] === "Converting to Docx");
    await sleep(1_200); // let the AOF fsync (everysec) catch up with the enqueue
    await redis.kill();
    await sleep(500);
    const restartedAt = Date.now();
    await redis.restart();
    const { last } = await s.waitFor(id, ["SUCCESS", "FAILURE"], 90_000);
    process.stderr.write(`TIMING redis kill/restart: job SUCCESS ${Date.now() - restartedAt} ms after Redis came back; stages seen ${JSON.stringify(s.states.get(id))}`);
    expect(last["state"]).toBe("SUCCESS");
    expect(isZip(documentPath(s.dirs.out, String(last["result"])))).toBe(true);
    // At-least-once: if Redis forgot the lock (the last second of writes before the kill), BullMQ may hand the job out again while the
    // first run is still going; both runs write the same bytes through an atomic rename, and the later scratch directory is removed when
    // that run ends. So "no scratch left" holds after a short settle, not at the instant SUCCESS is first visible.
    const scratch = (): string[] => (existsSync(join(s!.dirs.work, "jobs")) ? readdirSync(join(s!.dirs.work, "jobs")) : []);
    await until("scratch directories removed", () => scratch().length === 0, 15_000);
    const runs = (s.states.get(id) ?? []).filter((x) => x === "Receiving request").length;
    process.stderr.write(`TIMING redis kill/restart: the job ran ${runs} time(s) (${runs > 1 ? "duplicate execution after Redis lost the lock; one output" : "once"}); output files ${JSON.stringify(readdirSync(s.dirs.out))}\n`);
    expect(readdirSync(s.dirs.out).filter((f) => f.endsWith(".docx")).length).toBe(1);
  }, 150_000);
});
