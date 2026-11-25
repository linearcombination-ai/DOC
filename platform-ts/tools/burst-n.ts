/** N different documents at once (the four resources in N different orders and layouts): wall time, each job's time, failures. node tools/burst-n.ts <base url> <n=24> */
const base = process.argv[2] ?? "http://127.0.0.1:18301";
const n = Number(process.argv[3] ?? 24);
const types = ["ulb", "tn", "tq", "tw"];
const perms = (xs: string[]): string[][] => (xs.length <= 1 ? [xs] : xs.flatMap((x, i) => perms([...xs.slice(0, i), ...xs.slice(i + 1)]).map((p) => [x, ...p])));
const variants = perms(types).flatMap((p) => [false, true].map((print) => ({ p, print }))).slice(0, n);
const body = (v: { p: string[]; print: boolean }) => ({ email_address: null, assembly_strategy_kind: "lbo", layout_for_print: v.print, resource_requests: v.p.map((t) => ({ lang_code: "en", resource_type: t, book_code: "jud" })), generate_pdf: false, generate_epub: false, generate_docx: true, limit_words: true, document_request_source: "ui" });
const t0 = performance.now();
const results = await Promise.all(variants.map(async (v) => {
  const { task_id } = (await (await fetch(`${base}/documents_docx`, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(body(v)) })).json()) as { task_id: string };
  for (;;) {
    const s = (await (await fetch(`${base}/task_status/${task_id}`)).json()) as { state: string };
    if (s.state === "SUCCESS" || s.state === "FAILURE") return { state: s.state, ms: Math.round(performance.now() - t0) };
    await new Promise((r) => setTimeout(r, 50));
  }
}));
const ms = results.map((r) => r.ms).sort((a, b) => a - b);
console.log(JSON.stringify({ n, load1: Number(require_loadavg()), wallMs: Math.round(performance.now() - t0), failures: results.filter((r) => r.state !== "SUCCESS").length, p50: ms[Math.floor(n / 2)], p95: ms[Math.floor(n * 0.95)], max: ms.at(-1) }));
function require_loadavg(): string { return (process.getBuiltinModule("node:fs").readFileSync("/proc/loadavg", "utf8").split(" ")[0]) as string; }
