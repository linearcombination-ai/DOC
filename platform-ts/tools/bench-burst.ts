/** N distinct requests at once (warm assets): wall time until the last finishes, and each one's time. node tools/bench-burst.ts <base> <label> <body.json> <n> */
import { readFileSync, appendFileSync } from "node:fs";
const [base, label, bodyFile, nArg] = process.argv.slice(2);
if (!base || !label || !bodyFile || !nArg) throw new Error("usage: bench-burst.ts <base> <label> <body.json> <n>");
const n = Number(nArg);
const body = JSON.parse(readFileSync(bodyFile, "utf8")) as Record<string, unknown>;
const variants = Array.from({ length: n }, (_, i) => ({ ...body, use_chapter_labels: (i & 1) === 1, show_tn_book_intro: (i & 2) === 0, show_bc_book_intro: (i & 4) === 0 }));
async function one(v: Record<string, unknown>, t0: number): Promise<number> {
  const { task_id: id } = (await (await fetch(`${base}/documents_docx`, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(v) })).json()) as { task_id: string };
  for (;;) {
    const s = (await (await fetch(`${base}/task_status/${id}`)).json()) as { state: string };
    if (s.state === "SUCCESS" || s.state === "FAILURE") {
      if (s.state === "FAILURE") throw new Error("job failed");
      return Math.round(performance.now() - t0);
    }
    await new Promise((r) => setTimeout(r, 25));
  }
}
const load = readFileSync("/proc/loadavg", "utf8").split(" ")[0];
const t0 = performance.now();
const times = await Promise.all(variants.map((v) => one(v, t0)));
const line = { label, n, load1: Number(load), wallMs: Math.round(performance.now() - t0), eachMs: times.sort((a, b) => a - b) };
appendFileSync(process.env["BENCH_RESULT"] ?? `burst-${label}.jsonl`, `${JSON.stringify(line)}\n`);
console.log(JSON.stringify(line));
