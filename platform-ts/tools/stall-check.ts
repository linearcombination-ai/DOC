/** Does the 290 ms catalog normalisation (P4) matter in a worker? Event-loop lag while the full 21,000-row catalog is normalised, inline vs on the core thread. */
import { readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { monitorEventLoopDelay } from "node:perf_hooks";
import { CoreThread } from "../src/jobs/corethread.ts";
import { createCore, loadWasm } from "../src/core/wasm.ts";
import { silentLogger } from "../src/util/log.ts";

const pkg = resolve(import.meta.dirname, "../core/pkg");
const snap = process.argv[2] ?? "";
const rows = (JSON.parse(readFileSync(join(snap, "resp_primary.json"), "utf8")) as { data: { git_repo: unknown[] } }).data.git_repo;
const wasm = loadWasm(pkg);
const core = createCore(wasm, silentLogger());
const flags = core.flagPresets().fixed;
const keep = setInterval(() => undefined, 1000);

async function lag(label: string, work: () => Promise<void>): Promise<void> {
  const h = monitorEventLoopDelay({ resolution: 5 });
  h.enable();
  await new Promise((r) => setTimeout(r, 100));
  const s = performance.now();
  await work();
  const ms = performance.now() - s;
  await new Promise((r) => setTimeout(r, 50));
  h.disable();
  console.log(`${label.padEnd(44)} call ${ms.toFixed(0)} ms, max event-loop delay ${(h.max / 1e6).toFixed(0)} ms, p99 ${(h.percentile(99) / 1e6).toFixed(0)} ms`);
}
core.normalizeCatalog(rows, "primary", flags); // warm the module
await lag("normalize_catalog inline (main thread)", async () => void core.normalizeCatalog(rows, "primary", flags));
const thread = new CoreThread(pkg);
const payload = JSON.stringify({ rows, kind: "primary", flags });
await thread.call("normalize_catalog", payload, new AbortController().signal);
await lag("normalize_catalog on the core thread", async () => void (await thread.call("normalize_catalog", payload, new AbortController().signal)));
await thread.close();
clearInterval(keep);
