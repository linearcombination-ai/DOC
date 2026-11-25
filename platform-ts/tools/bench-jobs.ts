/**
 * Time a document job end to end against either implementation (DOC's FastAPI + Celery, or this shell + BullMQ): the same
 * POST, the same polling of GET /task_status/{id}, the same request body. Records, per run, when each distinct state was
 * first seen and when SUCCESS arrived; samples the resident memory of named process trees while jobs run.
 *
 *   node tools/bench-jobs.ts <base url> <label> <body.json> <out dir> <runs> [--cold-first] [--keep-output] [--procs name=pid,name=pid]
 */
import { execFileSync } from "node:child_process";
import { readFileSync, readdirSync, rmSync, writeFileSync, appendFileSync } from "node:fs";
import { join } from "node:path";

const [base, label, bodyFile, outDir, runsArg, ...flags] = process.argv.slice(2);
if (!base || !label || !bodyFile || !outDir || !runsArg) throw new Error("usage: bench-jobs.ts <base> <label> <body.json> <out dir> <runs> [--keep-output] [--procs name=pid,...]");
const runs = Number(runsArg);
const keepOutput = flags.includes("--keep-output");
const procsArg = flags.find((f) => f.startsWith("--procs="))?.slice(8) ?? "";
const resultFile = process.env["BENCH_RESULT"] ?? `bench-${label}.jsonl`;

function descendants(pid: number): number[] {
  try {
    const kids = execFileSync("pgrep", ["-P", String(pid)], { encoding: "utf8" }).split("\n").filter(Boolean).map(Number);
    return [pid, ...kids.flatMap(descendants)];
  } catch {
    return [pid];
  }
}
function rssKb(pid: number): number {
  try {
    const m = /VmRSS:\s+(\d+) kB/.exec(readFileSync(`/proc/${pid}/status`, "utf8"));
    return m ? Number(m[1]) : 0;
  } catch {
    return 0;
  }
}
const procs = procsArg ? procsArg.split(",").map((x) => ({ name: x.split("=")[0] as string, pid: Number(x.split("=")[1]) })) : [];
const peak: Record<string, number> = {};
const sampler = procs.length
  ? setInterval(() => {
      for (const p of procs) {
        const total = descendants(p.pid).reduce((a, b) => a + rssKb(b), 0);
        peak[p.name] = Math.max(peak[p.name] ?? 0, total);
      }
    }, 50)
  : undefined;

const body = readFileSync(bodyFile, "utf8");
for (let i = 0; i < runs; i += 1) {
  if (!keepOutput) for (const f of readdirSync(outDir)) rmSync(join(outDir, f), { force: true, recursive: true });
  const load = readFileSync("/proc/loadavg", "utf8").split(" ")[0];
  const t0 = performance.now();
  const res = await fetch(`${base}/documents_docx`, { method: "POST", headers: { "content-type": "application/json" }, body });
  const tPost = performance.now() - t0;
  const { task_id: id } = (await res.json()) as { task_id: string };
  const seen: [string, number][] = [];
  let result = "";
  for (;;) {
    const s = (await (await fetch(`${base}/task_status/${id}`)).json()) as { state: string; result?: unknown };
    if (seen.at(-1)?.[0] !== s.state) seen.push([s.state, Math.round(performance.now() - t0)]);
    if (s.state === "SUCCESS" || s.state === "FAILURE") {
      result = JSON.stringify(s.result ?? null);
      break;
    }
    if (performance.now() - t0 > 300_000) throw new Error("timeout");
    await new Promise((r) => setTimeout(r, 25));
  }
  const row = { label, run: i + 1, load1: Number(load), postMs: Math.round(tPost), totalMs: Math.round(performance.now() - t0), seen, result };
  appendFileSync(resultFile, `${JSON.stringify(row)}\n`);
  console.log(`${label} run ${i + 1}: ${row.totalMs} ms ${seen.map(([s, t]) => `${s}@${t}`).join(" > ")}`);
}
if (sampler) clearInterval(sampler);
writeFileSync(`${resultFile}.mem.json`, JSON.stringify({ label, peakRssKb: peak }, null, 1));
console.log(JSON.stringify({ peakRssKb: peak }));
