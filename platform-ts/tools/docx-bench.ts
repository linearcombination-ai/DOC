/**
 * Cost per job of the two docx seam modes, measured through the real pipeline (real Redis, real wasm core, real Python):
 *   node tools/docx-bench.ts [jobs=30]
 * Each job is a different document (a different order of the four resources gives a different cache key), so none is a cache
 * hit; jobs run one at a time so the numbers are the cost of one job, not of queueing. Prints, per mode: end-to-end time from
 * POST to SUCCESS, the `Converting to Docx` stage as the job logged it, and the Python worker's own generation time.
 */
import { createStack, judeRequest } from "../test/support/stack.ts";

const jobs = Number(process.argv[2] ?? 30);
const types = ["ulb", "tn", "tq", "tw"];
function permutations<T>(xs: T[]): T[][] {
  if (xs.length <= 1) return [xs];
  return xs.flatMap((x, i) => permutations([...xs.slice(0, i), ...xs.slice(i + 1)]).map((p) => [x, ...p]));
}
const keys = permutations(types).flatMap((p) => [false, true].map((print) => ({ order: p, print })));
const median = (xs: number[]): number => [...xs].sort((a, b) => a - b)[Math.floor(xs.length / 2)] ?? 0;
const p95 = (xs: number[]): number => [...xs].sort((a, b) => a - b)[Math.min(xs.length - 1, Math.floor(xs.length * 0.95))] ?? 0;

for (const mode of ["process", "persistent"]) {
  const s = await createStack({ env: { DOCX_MODE: mode, DOCX_POOL_SIZE: "1", WORKER_CONCURRENCY: "1" } });
  await s.startWorker();
  const e2e: number[] = [];
  const stage: number[] = [];
  const worker: number[] = [];
  for (let i = 0; i < jobs + 3; i += 1) {
    const k = keys[i % keys.length];
    const request = judeRequest({ layout_for_print: k?.print ?? false, resource_requests: (k?.order ?? types).map((t) => ({ lang_code: "en", resource_type: t, book_code: "jud" })) });
    const started = performance.now();
    const id = String((await s.post(request)).json["task_id"]);
    const done = await s.waitFor(id, ["SUCCESS", "FAILURE"], 60_000);
    const ms = performance.now() - started;
    if (done.last["state"] !== "SUCCESS") throw new Error(`job ${i} failed: ${JSON.stringify(done.last)}`);
    if (i < 3) continue; // warm-up: the first jobs pay for the wasm module, the Python start and the clones
    e2e.push(ms);
    const line = s.logs().filter((l) => l["msg"] === "job finished" && l["job"] === id).at(-1) as { timings?: Record<string, number> } | undefined;
    if (line?.timings?.["Converting to Docx"] !== undefined) stage.push(line.timings["Converting to Docx"]);
    const w = s.logs().filter((l) => l["msg"] === "document worker finished" && l["job"] === id).at(-1) as { workerMs?: number } | undefined;
    if (w?.workerMs !== undefined) worker.push(w.workerMs);
  }
  console.log(`${mode.padEnd(10)} n=${e2e.length}  end-to-end median ${median(e2e).toFixed(0)} ms (p95 ${p95(e2e).toFixed(0)})  docx stage median ${median(stage).toFixed(0)} ms (p95 ${p95(stage).toFixed(0)})  python generation median ${median(worker).toFixed(0)} ms`);
  await s.close();
}
process.exit(0);
