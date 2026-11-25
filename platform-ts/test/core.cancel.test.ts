/**
 * P6 items A3 and A4: the core's CPU stages are cancellable (a hung core call is killed and the job retried), and catalog
 * normalisation runs off any thread that must stay responsive.
 */
import { monitorEventLoopDelay } from "node:perf_hooks";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { createCore, loadWasm } from "../src/core/wasm.ts";
import { CoreThread } from "../src/jobs/corethread.ts";
import { silentLogger } from "../src/util/log.ts";
import { bigCatalogRows } from "./support/bigCatalog.ts";
import { createStack, PROTO, type Stack } from "./support/stack.ts";

let s: Stack | undefined;
afterEach(async () => {
  await s?.close();
  s = undefined;
});

/** Largest gap between ticks of a 5 ms timer while `work` runs, in ms. */
async function maxLagWhile(work: () => Promise<unknown>): Promise<number> {
  const h = monitorEventLoopDelay({ resolution: 5 });
  h.enable();
  await new Promise((r) => setTimeout(r, 50));
  await work();
  await new Promise((r) => setTimeout(r, 30));
  h.disable();
  return Math.round(h.max / 1e6);
}

describe("a hung core call is killed and the job is retried", () => {
  it("JavaScript spin inside the core thread on the first attempt: thread terminated, attempt 2 succeeds, the main thread stayed responsive", async () => {
    const marker = join(mkdtempSync(join(tmpdir(), "doc-spin-")), "once");
    s = await createStack({ env: { CORE_FAULT: `spin_once:${marker}`, STAGE_ASSEMBLE_DEADLINE_MS: "800" } });
    await s.startWorker();
    const lag = maxLagWhile(async () => {
      const id = String((await s?.post())?.json["task_id"]);
      return s?.waitFor(id, ["SUCCESS", "FAILURE"], 60_000).then((r) => {
        expect(r.last["state"]).toBe("SUCCESS");
        return id;
      });
    });
    const worst = await lag;
    process.stderr.write(`TIMING core spin (js): worst event-loop gap while the thread spun ${worst} ms\n`);
    expect(worst).toBeLessThan(600);
    expect(s.app.thread.terminated).toBe(1);
    const retry = s.logs().find((l) => l["msg"] === "attempt failed, will retry");
    expect(retry).toMatchObject({ code: "stage_deadline", transient: true, retrying: true });
    const ids = (await s.app.jobs?.service.queue.getJobs(["completed"])) ?? [];
    expect(ids[0]?.attemptsMade).toBeGreaterThanOrEqual(2);
  }, 90_000);

  it("a loop inside the wasm module itself (fault-injection build) is killed too: V8 terminate() interrupts running wasm", async () => {
    s = await createStack({ env: { CORE_FAULT: "wasm_spin", WASM_PKG_DIR: join(PROTO, "core/pkg-fault"), STAGE_ASSEMBLE_DEADLINE_MS: "800", JOB_ATTEMPTS: "2" } });
    await s.startWorker();
    const worst = await maxLagWhile(async () => {
      const id = String((await s?.post())?.json["task_id"]);
      const r = await s?.waitFor(id, ["SUCCESS", "FAILURE"], 60_000);
      expect(r?.last["state"]).toBe("FAILURE");
      expect(r?.last["result"]).toMatchObject({ code: "stage_deadline", retryable: true });
    });
    process.stderr.write(`TIMING core spin (wasm): worst event-loop gap ${worst} ms; thread terminated ${s.app.thread.terminated} times\n`);
    expect(s.app.thread.terminated).toBe(2); // one per attempt
    expect(worst).toBeLessThan(600);
  }, 90_000);
});

describe("catalog normalisation does not stall the event loop", () => {
  it("21,000 rows: inline blocks the loop for hundreds of ms; on the core thread the worst gap stays small", async () => {
    const rows = bigCatalogRows();
    const pkg = join(PROTO, "core/pkg-api");
    const wasm = loadWasm(pkg);
    const inline = createCore(wasm, silentLogger());
    const thread = new CoreThread(pkg);
    const offThread = createCore(wasm, silentLogger(), thread);
    const flags = inline.flagPresets().fixed;
    await inline.normalizeCatalog(rows, "primary", flags); // warm both paths
    await offThread.normalizeCatalog(rows, "primary", flags);
    const inlineGap = await maxLagWhile(() => inline.normalizeCatalog(rows, "primary", flags));
    const threadGap = await maxLagWhile(() => offThread.normalizeCatalog(rows, "primary", flags));
    process.stderr.write(`TIMING normalise 21,000 rows: worst event-loop gap inline ${inlineGap} ms, on the thread ${threadGap} ms\n`);
    expect(threadGap).toBeLessThan(inlineGap);
    expect(threadGap).toBeLessThan(150);
    const a = await inline.normalizeCatalog(rows, "primary", flags);
    const b = await offThread.normalizeCatalog(rows, "primary", flags);
    expect(b.repos.length).toBe(a.repos.length);
    await thread.close();
  }, 60_000);
});
