/**
 * Cross-process single flight for repository clones (P6 item A1): a Redis lock with a lease and fencing.
 * The count that matters is the number of `git clone` invocations: one per repository, however many workers ask at once.
 */
import { type ChildProcess } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Redis } from "ioredis";
import { afterEach, describe, expect, it } from "vitest";
import { Acquirer } from "../src/acquirer/acquirer.ts";
import { RepoLock } from "../src/jobs/repolock.ts";
import { MemoryMetrics } from "../src/obs/metrics.ts";
import { silentLogger } from "../src/util/log.ts";
import { createStack, judeRequest, type Stack } from "./support/stack.ts";

let s: Stack | undefined;
const extraDirs: string[] = [];
afterEach(async () => {
  await s?.close();
  s = undefined;
  for (const d of extraDirs.splice(0)) rmSync(d, { recursive: true, force: true });
});
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
async function until(what: string, test: () => Promise<boolean> | boolean, timeoutMs = 30_000): Promise<void> {
  const end = Date.now() + timeoutMs;
  while (Date.now() < end) {
    if (await test()) return;
    await sleep(25);
  }
  throw new Error(`timed out waiting for ${what}`);
}
const exited = (c: ChildProcess): Promise<number | null> => new Promise((r) => (c.exitCode !== null ? r(c.exitCode) : c.once("exit", (code) => r(code))));
const alternate = (): Record<string, unknown> => judeRequest({ layout_for_print: true }); // another key, the same two repositories

describe("two worker processes, one clone per repository", () => {
  it("with the lock: two workers, an empty cache and a slow clone make exactly 2 clones for 2 repositories", async () => {
    s = await createStack({ env: { WORKER_CONCURRENCY: "1", CLONE_LOCK_POLL_MS: "50" } });
    s.flaky.setMode("slow:1500");
    const a = s.spawnWorker();
    const b = s.spawnWorker();
    await until("two workers", async () => ((await s?.app.jobs?.service.health())?.workers ?? 0) >= 2);
    const [x, y] = [String((await s.post(judeRequest())).json["task_id"]), String((await s.post(alternate())).json["task_id"])];
    expect(x).not.toBe(y);
    const [rx, ry] = [await s.waitFor(x, ["SUCCESS", "FAILURE"], 90_000), await s.waitFor(y, ["SUCCESS", "FAILURE"], 90_000)];
    expect(rx.last["state"]).toBe("SUCCESS");
    expect(ry.last["state"]).toBe("SUCCESS");
    const clones = s.flaky.cloneCalls();
    process.stderr.write(`TIMING lock on: ${clones} git clone calls for 2 repositories, 2 workers\n`);
    expect(clones).toBe(2);
    for (const c of [a, b]) c.kill("SIGKILL");
    await Promise.all([exited(a), exited(b)]);
  }, 120_000);

  it("control, lock off: the same scenario clones every repository in every worker (4 clones for 2 repositories)", async () => {
    s = await createStack({ env: { WORKER_CONCURRENCY: "1", CLONE_LOCK: "false" } });
    s.flaky.setMode("slow:1500");
    const a = s.spawnWorker();
    const b = s.spawnWorker();
    await until("two workers", async () => ((await s?.app.jobs?.service.health())?.workers ?? 0) >= 2);
    const [x, y] = [String((await s.post(judeRequest())).json["task_id"]), String((await s.post(alternate())).json["task_id"])];
    await s.waitFor(x, ["SUCCESS", "FAILURE"], 90_000);
    await s.waitFor(y, ["SUCCESS", "FAILURE"], 90_000);
    const clones = s.flaky.cloneCalls();
    process.stderr.write(`TIMING lock off: ${clones} git clone calls for 2 repositories, 2 workers\n`);
    expect(clones).toBe(4);
    for (const c of [a, b]) c.kill("SIGKILL");
    await Promise.all([exited(a), exited(b)]);
  }, 120_000);

  it("the lock holder is SIGKILLed mid-clone: the lease expires, the other worker takes over and the document is produced", async () => {
    s = await createStack({ env: { WORKER_CONCURRENCY: "1", CLONE_LOCK_LEASE_MS: "1000", CLONE_LOCK_POLL_MS: "50", LOCK_DURATION_MS: "1500", STALLED_INTERVAL_MS: "500" } });
    s.flaky.setMode("slow:2500");
    const a = s.spawnWorker();
    await until("worker A", async () => ((await s?.app.jobs?.service.health())?.workers ?? 0) >= 1);
    const id = String((await s.post()).json["task_id"]);
    await until("A is cloning", () => s?.flaky.cloneCalls() !== 0);
    a.kill("SIGKILL");
    await exited(a);
    s.flaky.setMode("ok");
    const b = s.spawnWorker();
    const { last } = await s.waitFor(id, ["SUCCESS", "FAILURE"], 90_000);
    expect(last["state"]).toBe("SUCCESS");
    b.kill("SIGKILL");
    await exited(b);
  }, 120_000);
});

describe("lock mechanics", () => {
  function acquirerFor(st: Stack, root: string, lock: RepoLock, metrics = new MemoryMetrics()): Acquirer {
    return new Acquirer({ root, gitBin: st.flaky.bin, userAgent: "t", requestedWith: "t", stalenessMs: 60_000, cloneTimeoutMs: 20_000, maxAttempts: 1, backoff: { baseMs: 1, capMs: 1 }, concurrency: 4, gcGraceMs: 60_000, log: silentLogger(), lock, metrics, lockWaitMs: 20_000 });
  }
  const lockFor = (_st: Stack, scope: string, redis: Redis, leaseMs = 400): RepoLock => new RepoLock({ redis, prefix: "t", scope, leaseMs, pollMs: 25, log: silentLogger(), metrics: new MemoryMetrics() });

  it("fencing: a holder whose lease was lost (the key is gone, a later holder published) is fenced out and uses the newer copy instead of overwriting it", async () => {
    s = await createStack();
    const redis = new Redis(s.redis.url, { maxRetriesPerRequest: null });
    const root = mkdtempSync(join(tmpdir(), "doc-fence-"));
    extraDirs.push(root);
    const url = `${s.fx.root}/remotes/WycliffeAssociates/en_ulb`.replace(/^/, "file://");
    void url;
    const repoUrl = [...(await s.app.lookup.located("en", "fixed")).located].find((l) => l.resource_type === "ulb")?.url ?? "";
    expect(repoUrl).not.toBe("");
    const a = acquirerFor(s, root, lockFor(s, root, redis));
    const b = acquirerFor(s, root, lockFor(s, root, redis));
    await a.recover();
    s.flaky.setMode("slow:1500");
    const slow = a.acquire([repoUrl], Date.now() + 30_000);
    await until("A holds the lock", async () => (await redis.keys("t:lock:*")).length === 1);
    await redis.del(...(await redis.keys("t:lock:*"))); // the lease is lost (expired, partition, pause)
    s.flaky.setMode("ok");
    const fast = await b.acquire([repoUrl], Date.now() + 30_000);
    expect(fast.failed).toEqual([]);
    const late = await slow;
    expect(late.failed).toEqual([]);
    expect(b.stats.clones).toBe(1);
    expect(a.stats.clones).toBe(0);
    expect(a.stats.supersededClones).toBe(1);
    expect(late.acquired.get(repoUrl)?.path).toBe(fast.acquired.get(repoUrl)?.path); // both read the newer generation
    redis.disconnect();
  }, 60_000);

  it("Redis unavailable: the clone proceeds without the lock (a request must end up with a document)", async () => {
    s = await createStack();
    const dead = new Redis(1, "127.0.0.1", { enableOfflineQueue: false, maxRetriesPerRequest: 0, retryStrategy: () => null, lazyConnect: true });
    dead.on("error", () => undefined);
    const root = mkdtempSync(join(tmpdir(), "doc-nolock-"));
    extraDirs.push(root);
    const metrics = new MemoryMetrics();
    const repoUrl = [...(await s.app.lookup.located("en", "fixed")).located].find((l) => l.resource_type === "tn")?.url ?? "";
    const a = acquirerFor(s, root, lockFor(s, root, dead), metrics);
    await a.recover();
    const out = await a.acquire([repoUrl], Date.now() + 30_000);
    expect(out.failed).toEqual([]);
    expect(a.stats.clones).toBe(1);
  }, 30_000);
});
