/** The status contract and the happy path: the legacy state strings in order, SUCCESS with the key, a valid docx placed atomically. */
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { createStack, type Stack } from "./support/stack.ts";
import { documentPath } from "../src/jobs/pipeline.ts";
import { STAGE_ORDER } from "../src/jobs/types.ts";

let s: Stack;
beforeAll(async () => {
  s = await createStack();
  await s.startWorker();
}, 60_000);
afterAll(async () => {
  await s.close();
});

describe("document job, golden path", () => {
  it("answers PENDING for an unknown task id, as Celery does", async () => {
    expect(await s.status("does-not-exist")).toEqual({ state: "PENDING" });
  });

  it("walks DOC's exact state strings in order, then SUCCESS with the document request key", async () => {
    const posted = await s.post();
    expect(posted.status).toBe(200);
    const id = String(posted.json["task_id"]);
    const { trace, last } = await s.waitFor(id);
    // The order as the worker published it (every update, none missed), and then what a poller saw: a subsequence of it.
    await new Promise((r) => setTimeout(r, 100));
    expect(s.states.get(id)).toEqual(STAGE_ORDER.slice(0, 6));
    const polled = trace.filter((t) => t !== "PENDING" && t !== "SUCCESS");
    expect(polled.every((t, i) => i === 0 || STAGE_ORDER.indexOf(t as never) > STAGE_ORDER.indexOf(polled[i - 1] as never))).toBe(true);
    expect(trace.at(-1)).toBe("SUCCESS");
    expect(last["state"]).toBe("SUCCESS");
    const key = String(last["result"]);
    expect(key).toMatch(/^en_ulb-tn_jud_[0-9a-f]{12}$/);
    const file = documentPath(s.dirs.out, key);
    expect(existsSync(file)).toBe(true);
    const bytes = readFileSync(file);
    expect(bytes.subarray(0, 2).toString()).toBe("PK");
    expect(bytes.byteLength).toBeGreaterThan(10_000);
    // nothing but the document and its findings were left in the output directory, and no scratch directory remains
    expect(readdirSync(s.dirs.out).sort()).toEqual([`${key}.docx`, `${key}.findings.json`].sort());
    expect(readdirSync(join(s.dirs.work, "jobs"))).toEqual([]);
  }, 60_000);
});
