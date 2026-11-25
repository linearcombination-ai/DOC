/** Regression: a poll must never see a half-written terminal state (SUCCESS without a result, FAILURE without a reason). */
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { createStack, judeRequest, type Stack } from "./support/stack.ts";
import { taskStatusFrom } from "../src/jobs/queue.ts";

let s: Stack;
beforeAll(async () => {
  s = await createStack({ env: { WORKER_CONCURRENCY: "4" } });
  await s.startWorker();
}, 60_000);
afterAll(async () => {
  await s.close();
});

describe("GET /task_status under hammering", () => {
  it("never answers SUCCESS without the key, nor FAILURE without a reason, while jobs complete and fail around it", async () => {
    // prime: one real job so later identical requests end at the cache check within a few ms (many completions per second)
    const first = await s.post();
    await s.waitFor(String(first.json["task_id"]));
    const bad: string[] = [];
    let polls = 0;
    const ids: string[] = [];
    const spawnJobs = async (): Promise<void> => {
      for (let i = 0; i < 120; i += 1) {
        const r = i % 4 === 3 ? judeRequest({ resource_requests: [{ lang_code: "en", resource_type: "ulb", book_code: "3jn" }] }) : judeRequest();
        ids.push(String((await s.post(r)).json["task_id"]));
        await new Promise((res) => setTimeout(res, 3));
      }
    };
    let running = true;
    const poll = async (): Promise<void> => {
      while (running) {
        for (const id of [...ids]) {
          const st = await s.status(id);
          polls += 1;
          if (st["state"] === "SUCCESS" && !(typeof st["result"] === "string" && /^en_ulb-tn_jud_[0-9a-f]{12}$/.test(st["result"]))) bad.push(JSON.stringify(st));
          if (st["state"] === "FAILURE" && (typeof st["result"] !== "object" || st["result"] === null || (st["result"] as { code?: string }).code === "internal")) bad.push(JSON.stringify(st));
        }
        await new Promise((res) => setTimeout(res, 0));
      }
    };
    const pollers = [poll(), poll(), poll()];
    await spawnJobs();
    await Promise.all(ids.map((id) => s.waitFor(id, ["SUCCESS", "FAILURE"], 120_000)));
    running = false;
    await Promise.all(pollers);
    expect(polls).toBeGreaterThan(100);
    expect(bad).toEqual([]);
  }, 180_000);
});

describe("the race itself, simulated", () => {
  const stale = { progress: { state: "Converting to Docx", attempt: 1 }, returnvalue: null, failedReason: undefined };
  it("a job that completes between the two reads is still answered with its key", async () => {
    // the record as it was while the job was still active is what a read made *before* the state read returns;
    // the job completes at the instant of the state read, so any read made after it sees the result
    let completed = false;
    const queue = {
      getJobState: async () => {
        completed = true;
        return "completed";
      },
      getJob: async () => (completed ? { progress: stale.progress, returnvalue: "en_ulb_jud_aaaaaaaaaaaa", failedReason: undefined } : stale),
    };
    expect(await taskStatusFrom(queue, "1")).toEqual({ state: "SUCCESS", result: "en_ulb_jud_aaaaaaaaaaaa" });
  });
  it("a job that fails between the two reads is still answered with its reason", async () => {
    const reason = JSON.stringify({ code: "stage_deadline", message: "x", retryable: true });
    const queue = { getJobState: async () => "failed", getJob: async () => ({ progress: null, returnvalue: null, failedReason: reason }) };
    expect(await taskStatusFrom(queue, "1")).toEqual({ state: "FAILURE", result: { code: "stage_deadline", message: "x", retryable: true } });
  });
  it("PENDING for an unknown id, the stage for a running job", async () => {
    expect(await taskStatusFrom({ getJobState: async () => "unknown", getJob: async () => undefined }, "x")).toEqual({ state: "PENDING" });
    expect(await taskStatusFrom({ getJobState: async () => "active", getJob: async () => stale }, "x")).toEqual({ state: "Converting to Docx" });
  });
});
