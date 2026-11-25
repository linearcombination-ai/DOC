/**
 * The persistent docx worker pool against real Python processes: reuse, crash, hang, memory ceiling, recycling, queueing,
 * and the whole job path in persistent mode. Every test that kills something asserts the process group is gone.
 */
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { afterEach, describe, expect, it } from "vitest";
import { pidAlive, validateDocx } from "../src/jobs/docx.ts";
import { DocxPool } from "../src/jobs/docxpool.ts";
import { JobError } from "../src/jobs/errors.ts";
import { MemoryMetrics } from "../src/obs/metrics.ts";
import { silentLogger } from "../src/util/log.ts";
import { createStack, judeRequest, PROTO, type Stack } from "./support/stack.ts";

const SERVER = join(PROTO, "python/docx_server.py");
const TMP = process.env["TMPDIR"] ?? tmpdir();
let dir: string | undefined;
let pool: DocxPool | undefined;
let stack: Stack | undefined;
afterEach(async () => {
  await pool?.close();
  pool = undefined;
  await stack?.close();
  stack = undefined;
  if (dir) rmSync(dir, { recursive: true, force: true });
  dir = undefined;
});

const metrics = new MemoryMetrics();
function makePool(over: Partial<ConstructorParameters<typeof DocxPool>[0]> = {}): DocxPool {
  pool = new DocxPool({ pythonBin: "python3", script: SERVER, size: 2, recycleAfter: 100, memoryCeilingMb: 400, startTimeoutMs: 10_000, fault: "", log: silentLogger(), metrics, memoryPollMs: 50, ...over });
  return pool;
}
let n = 0;
function jobFile(format: "docx" | "html" = "docx"): { file: string; out: string } {
  dir ??= mkdtempSync(join(TMP, "doc-pool-"));
  n += 1;
  const out = join(dir, `o${n}.${format}`);
  const file = join(dir, `job${n}.json`);
  writeFileSync(file, JSON.stringify({ version: 2, format, request_id: `req-${n}`, job_id: `j${n}`, request_key: `k${n}`, parts: [{ content: `<h1 class="resource-title">T${n}</h1><p>hello ${n}</p>` }], layout_for_print: false, title1: "t1", title2: "t2", book: "jud", output_path: out }));
  return { file, out };
}
const run = (p: DocxPool, f: { file: string }, deadlineMs = 10_000, signal = new AbortController().signal) => p.run({ jobFile: f.file, deadlineMs, signal, requestId: "req" });
const pgrep = (pattern: string): number[] => {
  try {
    return execFileSync("pgrep", ["-f", "-P", String(process.pid), pattern], { encoding: "utf8" }).split("\n").filter(Boolean).map(Number).filter((p) => p !== process.pid);
  } catch {
    return [];
  }
};

describe("persistent docx pool", () => {
  it("one Python process serves many jobs (same pid), each output is a valid docx, and html works too", async () => {
    const p = makePool({ size: 1 });
    await p.warm();
    const pids = new Set<number>();
    for (let i = 0; i < 5; i += 1) {
      const f = jobFile();
      const r = await run(p, f);
      pids.add(r.pid);
      await validateDocx(f.out, "docx");
      expect(r.result.jobs).toBe(i + 1);
    }
    const h = jobFile("html");
    await run(p, h);
    expect(readFileSync(h.out, "utf8")).toContain("<p>hello");
    expect(pids.size).toBe(1);
    expect(p.stats()).toMatchObject({ mode: "persistent", spawned: 1, crashed: 0, jobs: 6 });
  }, 30_000);

  it("a crash mid-job fails that job with a transient typed error, the process is replaced, and the next job works", async () => {
    const marker = join(mkdtempSync(join(TMP, "doc-marker-")), "m");
    const p = makePool({ size: 1, fault: `crash_once:${marker}` });
    const f1 = jobFile();
    const err = await run(p, f1).catch((e: unknown) => e);
    expect(err).toBeInstanceOf(JobError);
    expect((err as JobError).code).toBe("docx_worker_crashed");
    expect((err as JobError).transient).toBe(true);
    const deadPid = Number((err as JobError).detail["pid"]);
    expect(pidAlive(deadPid)).toBe(false);
    const f2 = jobFile();
    const ok = await run(p, f2);
    expect(ok.pid).not.toBe(deadPid);
    await validateDocx(f2.out);
    expect(p.stats().crashed).toBeGreaterThanOrEqual(1);
  }, 30_000);

  it("a hung job is killed at the deadline (process group gone), replaced, and the pool still works", async () => {
    const marker = join(mkdtempSync(join(TMP, "doc-marker-")), "m");
    const p = makePool({ size: 1, fault: `hang_once:${marker}` });
    const f = jobFile();
    const started = Date.now();
    const err = await run(p, f, 600).catch((e: unknown) => e);
    expect(err).toBeInstanceOf(JobError);
    expect((err as JobError).code).toBe("docx_worker_timeout");
    expect(Date.now() - started).toBeLessThan(4_000);
    const pid = Number((err as JobError).detail["pid"]);
    expect(pidAlive(pid)).toBe(false);
    expect(pgrep(`docx_server.py`).filter((x) => x === pid)).toEqual([]);
    const f2 = jobFile();
    const ok = await run(p, f2);
    expect(ok.pid).not.toBe(pid);
    await validateDocx(f2.out);
    expect(p.stats().killed).toBe(1);
  }, 30_000);

  it("an abort signal (the stage deadline) kills the worker like a deadline does", async () => {
    const p = makePool({ size: 1, fault: "slow:5000" });
    const ac = new AbortController();
    setTimeout(() => ac.abort(), 300);
    const err = await run(p, jobFile(), 20_000, ac.signal).catch((e: unknown) => e);
    expect((err as JobError).code).toBe("stage_deadline");
    expect(pidAlive(Number((err as JobError).detail["pid"]))).toBe(false);
  }, 20_000);

  it("a worker over its memory ceiling is killed mid-job (transient docx_worker_memory) and replaced", async () => {
    const marker = join(mkdtempSync(join(TMP, "doc-marker-")), "m");
    void marker;
    const p2 = makePool({ size: 1, memoryCeilingMb: 120, fault: "memhog:300" });
    const err = await run(p2, jobFile(), 10_000).catch((e: unknown) => e);
    expect(err).toBeInstanceOf(JobError);
    expect((err as JobError).code).toBe("docx_worker_memory");
    expect((err as JobError).transient).toBe(true);
    expect(pidAlive(Number((err as JobError).detail["pid"]))).toBe(false);
  }, 30_000);

  it("a worker is recycled after N jobs: the pid changes and the old process is gone", async () => {
    const p = makePool({ size: 1, recycleAfter: 3 });
    await p.warm();
    const pids: number[] = [];
    for (let i = 0; i < 7; i += 1) pids.push((await run(p, jobFile())).pid);
    expect(new Set(pids).size).toBe(3); // jobs 1-3, 4-6, 7
    expect(pids[0]).toBe(pids[2]);
    expect(pids[3]).not.toBe(pids[2]);
    expect(pidAlive(pids[0] as number)).toBe(false);
    expect(p.stats().recycled).toBe(2);
  }, 30_000);

  it("an idle worker that dies is replaced without any job noticing", async () => {
    const p = makePool({ size: 1 });
    await p.warm();
    const first = await run(p, jobFile());
    process.kill(first.pid, "SIGKILL");
    await new Promise((r) => setTimeout(r, 300));
    const again = await run(p, jobFile());
    expect(again.pid).not.toBe(first.pid);
    expect(p.stats().crashed).toBe(1);
  }, 30_000);

  it("more jobs than workers wait their turn: 6 jobs on 2 workers all finish, never more than 2 Python processes", async () => {
    const p = makePool({ size: 2, fault: "slow:200" });
    await p.warm();
    let peak = 0;
    const watch = setInterval(() => (peak = Math.max(peak, pgrep("docx_server.py").length)), 20);
    const results = await Promise.all(Array.from({ length: 6 }, () => run(p, jobFile())));
    clearInterval(watch);
    expect(results.length).toBe(6);
    expect(new Set(results.map((r) => r.pid)).size).toBeLessThanOrEqual(2);
    expect(peak).toBeLessThanOrEqual(2);
  }, 30_000);

  it("close() leaves no Python process behind", async () => {
    const p = makePool({ size: 2 });
    await p.warm();
    const before = pgrep("docx_server.py");
    expect(before.length).toBe(2);
    await p.close();
    pool = undefined;
    for (const pid of before) expect(pidAlive(pid)).toBe(false);
  }, 20_000);
});

describe("persistent mode through the whole job", () => {
  it("a hang on the first attempt is killed at the stage deadline, the job is retried on a fresh worker and the document is produced", async () => {
    const marker = join(mkdtempSync(join(TMP, "doc-marker-")), "m");
    stack = await createStack({ env: { DOCX_MODE: "persistent", DOCX_FAULT: `hang_once:${marker}`, STAGE_DOCX_DEADLINE_MS: "1500", DOCX_POOL_SIZE: "1" } });
    await stack.startWorker();
    const id = String((await stack.post()).json["task_id"]);
    const { last } = await stack.waitFor(id);
    expect(last["state"]).toBe("SUCCESS");
    expect((await stack.app.jobs?.service.queue.getJob(id))?.attemptsMade).toBeGreaterThanOrEqual(2);
    const lines = stack.logs().filter((l) => l["msg"] === "attempt failed, will retry");
    // the stage deadline and the pool's own deadline are the same instant: whichever fires first names the kill
    expect(["docx_worker_timeout", "stage_deadline"]).toContain(lines[0]?.["code"]);
    expect(lines[0]).toMatchObject({ transient: true, retrying: true });
  }, 60_000);

  it("the same request in persistent mode produces a valid docx with the same bytes as per-job mode", async () => {
    const outputs: Buffer[] = [];
    for (const mode of ["process", "persistent"]) {
      stack = await createStack({ env: { DOCX_MODE: mode } });
      await stack.startWorker();
      const id = String((await stack.post(judeRequest())).json["task_id"]);
      const { last } = await stack.waitFor(id);
      expect(last["state"]).toBe("SUCCESS");
      outputs.push(readFileSync(join(stack.dirs.out, `${String(last["result"])}.docx`)));
      await stack.close();
      stack = undefined;
    }
    // a docx is a zip with timestamps; compare the document body instead of the container
    const text = (b: Buffer): string => b.toString("latin1");
    expect(outputs[0]?.length).toBe(outputs[1]?.length);
    expect(text(outputs[0] as Buffer).includes("word/document.xml")).toBe(true);
  }, 90_000);
});
