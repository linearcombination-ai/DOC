/**
 * Fault injection for the job path, in process (a real Redis, the real wasm core, the real Python worker, real git):
 * transient versus permanent errors, retries with backoff, hung stages killed, duplicate coalescing, isolation.
 */
import { execFileSync } from "node:child_process";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { createStack, judeRequest, type Stack } from "./support/stack.ts";
import { backoffMs } from "../src/jobs/queue.ts";
import { loadJobConfig } from "../src/jobs/config.ts";
import { documentPath } from "../src/jobs/pipeline.ts";
import { ProcessDocxRunner, pidAlive } from "../src/jobs/docx.ts";
import { noMetrics } from "../src/obs/metrics.ts";
import { JobError } from "../src/jobs/errors.ts";
import { silentLogger } from "../src/util/log.ts";
import { PROTO } from "./support/stack.ts";

let s: Stack | undefined;
afterEach(async () => {
  await s?.close();
  s = undefined;
});

const pgrep = (pattern: string): number[] => {
  try {
    return execFileSync("pgrep", ["-f", pattern], { encoding: "utf8" }).split("\n").filter(Boolean).map(Number).filter((p) => p !== process.pid);
  } catch {
    return [];
  }
};
const failure = (last: Record<string, unknown>): { code: string; message: string; retryable: boolean } => last["result"] as { code: string; message: string; retryable: boolean };
async function attemptsMade(st: Stack, id: string): Promise<number> {
  return (await st.app.jobs?.service.queue.getJob(id))?.attemptsMade ?? -1;
}

describe("errors: transient is retried, permanent is not", () => {
  it("a catalog outage longer than the client's own retries is retried by the job and the document is still produced", async () => {
    s = await createStack();
    s.api.setMode({ kind: "down" });
    await s.startWorker();
    const id = String((await s.post()).json["task_id"]);
    // wait for the first attempt to give up, then heal the data API
    for (let i = 0; i < 400 && (await attemptsMade(s, id)) < 1; i += 1) await new Promise((r) => setTimeout(r, 25));
    expect(await attemptsMade(s, id)).toBeGreaterThanOrEqual(1);
    s.api.setMode({ kind: "ok" });
    const { last } = await s.waitFor(id);
    expect(last["state"]).toBe("SUCCESS");
    expect(await attemptsMade(s, id)).toBeGreaterThanOrEqual(2);
    const retried = s.logs().filter((l) => l["msg"] === "attempt failed, will retry");
    expect(retried.length).toBeGreaterThanOrEqual(1);
    expect(retried[0]).toMatchObject({ code: "catalog_unavailable", transient: true, retrying: true });
    expect(existsSync(documentPath(s.dirs.out, String(last["result"])))).toBe(true);
  }, 90_000);

  it("a permanent error (the book is not in the repository) is not retried and carries {code, message, retryable:false}", async () => {
    s = await createStack();
    await s.startWorker();
    const body = judeRequest({ resource_requests: [{ lang_code: "en", resource_type: "ulb", book_code: "3jn" }] });
    const id = String((await s.post(body)).json["task_id"]);
    const { last } = await s.waitFor(id);
    expect(last["state"]).toBe("FAILURE");
    expect(failure(last), JSON.stringify(last)).toMatchObject({ code: "resource_unavailable", retryable: false });
    expect(await attemptsMade(s, id)).toBe(1);
    expect(s.logs().filter((l) => l["msg"] === "attempt failed, giving up").length).toBe(1);
    expect(readdirSync(s.dirs.out)).toEqual([]);
  }, 60_000);

  it("an invalid request never reaches the queue: the legacy 422 body", async () => {
    s = await createStack();
    const bad = await s.post({ resource_requests: [] });
    expect(bad.status).toBe(422);
    expect(bad.json).toMatchObject({ status_code: 10422, data: null });
    expect((await s.app.jobs?.service.health())?.waiting).toBe(0);
  });

  it("every attempt of a failing job writes one structured line, then one terminal line; backoff has jitter and a ceiling", async () => {
    s = await createStack({ env: { DOCX_FAULT: "crash", JOB_ATTEMPTS: "3" } });
    await s.startWorker();
    const id = String((await s.post()).json["task_id"]);
    const { last } = await s.waitFor(id);
    expect(failure(last), JSON.stringify(last)).toMatchObject({ code: "docx_worker_crashed", retryable: true });
    const lines = s.logs().filter((l) => String(l["msg"]).startsWith("attempt failed"));
    expect(lines.map((l) => [l["attempt"], l["retrying"], l["msg"]])).toEqual([[1, true, "attempt failed, will retry"], [2, true, "attempt failed, will retry"], [3, false, "attempt failed, giving up"]]);
    const cfg = loadJobConfig({ JOB_BACKOFF_BASE_MS: "1000", JOB_BACKOFF_CAP_MS: "8000" });
    const draws = Array.from({ length: 200 }, (_, i) => backoffMs(cfg, 4, () => i / 200));
    expect(Math.min(...draws)).toBeGreaterThanOrEqual(500);
    expect(Math.max(...draws)).toBeLessThanOrEqual(8000);
    expect(new Set(draws).size).toBeGreaterThan(100); // jitter: not one fixed delay
    expect(backoffMs(cfg, 20, () => 0.999)).toBeLessThanOrEqual(8000); // the cap holds at any attempt
  }, 60_000);
});

describe("deadlines kill hung work", () => {
  it("a hung docx worker (ignoring SIGTERM) is killed at the stage deadline and its PID is gone", async () => {
    const dir = (await import("node:fs")).mkdtempSync(join(process.env["TMPDIR"] ?? "/tmp", "docx-hang-"));
    const jobFile = join(dir, "job.json");
    (await import("node:fs")).writeFileSync(jobFile, JSON.stringify({ version: 1, job_id: "x", request_key: "k", parts: [{ content: "<p>x</p>" }], layout_for_print: false, title1: "t", title2: "t", book: "jud", output_path: join(dir, "o.docx") }));
    const started = Date.now();
    const err = await new ProcessDocxRunner({ pythonBin: "python3", script: join(PROTO, "python/docx_worker.py"), fault: "hang", log: silentLogger(), metrics: noMetrics }).run({ jobFile, deadlineMs: 700, signal: new AbortController().signal, requestId: "t" }).catch((e: unknown) => e);
    expect(err).toBeInstanceOf(JobError);
    const je = err as JobError;
    expect(je.code).toBe("docx_worker_timeout");
    expect(je.transient).toBe(true);
    expect(Date.now() - started).toBeLessThan(3_000);
    const pid = Number(je.detail["pid"]);
    expect(pid).toBeGreaterThan(1);
    expect(pidAlive(pid)).toBe(false);
    expect(pgrep(`docx_worker.py ${jobFile}`)).toEqual([]);
  }, 20_000);

  it("through the whole job: a hung docx worker fails the job after the attempts with a retryable code and nothing is left running", async () => {
    s = await createStack({ env: { DOCX_FAULT: "hang", STAGE_DOCX_DEADLINE_MS: "600", JOB_ATTEMPTS: "2" } });
    await s.startWorker();
    const id = String((await s.post()).json["task_id"]);
    const { last } = await s.waitFor(id);
    expect(failure(last), JSON.stringify(last)).toMatchObject({ code: "stage_deadline", retryable: true }); // the stage deadline fired first and aborted the worker call (the worker-level deadline is tested above)
    expect(pgrep(`docx_worker.py ${s.dirs.work}`)).toEqual([]);
    expect(readdirSync(join(s.dirs.work, "jobs"))).toEqual([]); // the scratch directory was removed
  }, 60_000);

  it("a hung git clone fails the provisioning stage at its deadline; no git process is left", async () => {
    s = await createStack({ env: { STAGE_PROVISION_DEADLINE_MS: "1500", GIT_CLONE_TIMEOUT_MS: "800", GIT_MAX_ATTEMPTS: "1", JOB_ATTEMPTS: "1" } });
    s.flaky.setMode("hang");
    await s.startWorker();
    const id = String((await s.post()).json["task_id"]);
    const { last } = await s.waitFor(id);
    expect(last["state"]).toBe("FAILURE");
    expect(failure(last).retryable).toBe(true);
    expect(["resource_unavailable", "stage_deadline"]).toContain(failure(last).code);
    await new Promise((r) => setTimeout(r, 300));
    expect(pgrep("[s]leep 600")).toEqual([]); // the flaky git wrapper's hang mode is `exec sleep 600`: none may survive the kill
  }, 60_000);

  it("a core call that never returns is killed with its thread at the deadline, and the next call gets a fresh thread", async () => {
    s = await createStack({ env: { CORE_FAULT: "spin", STAGE_ASSEMBLE_DEADLINE_MS: "700", JOB_ATTEMPTS: "1" } });
    await s.startWorker();
    const lagSamples: number[] = [];
    let last = performance.now();
    const timer = setInterval(() => {
      const now = performance.now();
      lagSamples.push(now - last);
      last = now;
    }, 20);
    const id = String((await s.post()).json["task_id"]);
    const done = await s.waitFor(id);
    clearInterval(timer);
    expect(failure(done.last), JSON.stringify(done.last)).toMatchObject({ code: "stage_deadline", retryable: true });
    expect(s.app.thread.terminated).toBeGreaterThanOrEqual(1);
    // the spin ran on the thread, so this process's event loop kept turning (worst gap well below the 700 ms deadline)
    expect(Math.max(...lagSamples)).toBeLessThan(400);
    // a fresh thread answers the next call
    const ok = await s.app.jobCore.parse({ request: judeRequest(), dtos: [], tree: {}, usfm: [] }, new AbortController().signal);
    expect(ok.title1).toBeTypeOf("string");
  }, 60_000);
});

describe("duplicates and concurrency", () => {
  it("identical in-flight requests coalesce to one job and one output; a different request gets its own job", async () => {
    s = await createStack({ env: { DOCX_FAULT: "slow:700" } });
    await s.startWorker();
    const posts = await Promise.all([1, 2, 3, 4, 5].map(() => s!.post()));
    const ids = new Set(posts.map((p) => String(p.json["task_id"])));
    expect(ids.size).toBe(1);
    expect(posts.filter((p) => p.headers.get("x-doc-deduplicated") === "1").length).toBe(4);
    const other = await s.post(judeRequest({ use_chapter_labels: true }));
    expect(String(other.json["task_id"])).not.toBe([...ids][0]);
    const [a, b] = await Promise.all([s.waitFor([...ids][0] as string), s.waitFor(String(other.json["task_id"]))]);
    expect(a.last["state"]).toBe("SUCCESS");
    expect(b.last["state"]).toBe("SUCCESS");
    expect(a.last["result"]).not.toBe(b.last["result"]);
    expect(readdirSync(s.dirs.out).filter((f) => f.endsWith(".docx")).length).toBe(2);
    const counts = await s.app.jobs?.service.queue.getJobCounts("completed", "failed");
    expect(counts).toMatchObject({ completed: 2, failed: 0 });
    // a repeat after completion is a new job that ends at the cache check, without converting again
    const again = await s.post();
    expect(String(again.json["task_id"])).not.toBe([...ids][0]);
    await s.waitFor(String(again.json["task_id"]));
    expect(s.states.get(String(again.json["task_id"]))).toEqual(["Receiving request"]);
  }, 60_000);

  it("concurrent jobs never share a scratch directory, and each document is the one that was asked for", async () => {
    s = await createStack({ env: { DOCX_FAULT: "slow:800", WORKER_CONCURRENCY: "3" } });
    await s.startWorker();
    const bodies = [judeRequest(), judeRequest({ use_chapter_labels: true }), judeRequest({ show_tn_book_intro: false })];
    const ids = await Promise.all(bodies.map(async (b) => String((await s!.post(b)).json["task_id"])));
    // while all three convert, look at the scratch root
    let seen: string[] = [];
    for (let i = 0; i < 200 && seen.length < 3; i += 1) {
      await new Promise((r) => setTimeout(r, 20));
      seen = existsSync(join(s.dirs.work, "jobs")) ? readdirSync(join(s.dirs.work, "jobs")) : [];
    }
    expect(seen.length).toBe(3);
    expect(new Set(seen).size).toBe(3);
    const results = await Promise.all(ids.map((id) => s!.waitFor(id)));
    const keys = results.map((r) => String(r.last["result"]));
    expect(new Set(keys).size).toBe(3);
    for (const k of keys) expect(readFileSync(documentPath(s.dirs.out, k)).subarray(0, 2).toString()).toBe("PK");
    expect(readdirSync(join(s.dirs.work, "jobs"))).toEqual([]);
  }, 90_000);
});

describe("the key", () => {
  it("matches DOC's golden hashes (tests/unit/test_document_generator.py:214,229)", async () => {
    s = await createStack();
    const core = s.app.jobCore;
    const a = core.key({ assembly_strategy_kind: "lbo", resource_requests: [{ lang_code: "en", resource_type: "ulb", book_code: "mat" }] });
    expect(a).toMatchObject({ ok: true, key: "en_ulb_mat_70c7420bb621" });
    const b = core.key({ assembly_strategy_kind: "lbo", assembly_layout_kind: "1c", document_request_source: "ui", limit_words: false, resource_requests: [{ lang_code: "en", resource_type: "ulb", book_code: "mat" }, { lang_code: "en", resource_type: "tw", book_code: "mat" }] });
    expect(b).toMatchObject({ ok: true, key: "en_ulb-tw_mat_50944a0aa352" });
  });
});
