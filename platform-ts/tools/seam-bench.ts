/**
 * What the Python seam costs per job, measured on a real DocxJob (Jude, ULB + TN) built from the test fixtures:
 * serialisation, interpreter start, imports, generation, validation, with and without bytecode caching, system and venv python.
 *   node tools/seam-bench.ts [python bin ...]
 */
import { execFileSync, spawnSync } from "node:child_process";
import { mkdirSync, readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { CoreThread } from "../src/jobs/corethread.ts";
import { JobCore } from "../src/jobs/jobcore.ts";
import { loadWasm } from "../src/core/wasm.ts";
import { DocxJob, validateDocx } from "../src/jobs/docx.ts";

const root = resolve(import.meta.dirname, "..");
const pkg = join(root, "core/pkg");
const content = join(root, "test/fixtures/content");
const tmp = join(process.env["TMPDIR"] ?? "/tmp", "seam-bench");
mkdirSync(tmp, { recursive: true });

function tree(dir: string, rel = ""): { files: string[]; texts: Record<string, string> } {
  const out = { files: [] as string[], texts: {} as Record<string, string> };
  for (const n of readdirSync(join(dir, rel))) {
    const p = rel ? `${rel}/${n}` : n;
    if (statSync(join(dir, p)).isDirectory()) {
      const sub = tree(dir, p);
      out.files.push(...sub.files);
      Object.assign(out.texts, sub.texts);
    } else {
      out.files.push(p);
      if (/\.(md|txt)$/.test(n)) out.texts[p] = readFileSync(join(dir, p), "utf8");
    }
  }
  return out;
}
const t = (label: string, ms: number) => console.log(`${label.padEnd(46)} ${ms.toFixed(1)} ms`);
const time = async <T>(f: () => Promise<T> | T): Promise<[T, number]> => {
  const s = performance.now();
  const v = await f();
  return [v, performance.now() - s];
};

const keepAlive = setInterval(() => undefined, 1000); // the core thread is unref()ed (it must not hold a real worker open), so hold this script open
const thread = new CoreThread(pkg);
const core = new JobCore(loadWasm(pkg), thread);
const request = { assembly_strategy_kind: "lbo", document_request_source: "ui", generate_docx: true, resource_requests: [{ lang_code: "en", resource_type: "ulb", book_code: "jud" }, { lang_code: "en", resource_type: "tn", book_code: "jud" }] };
const signal = new AbortController().signal;
const input = { request, dtos: [{ lang_code: "en", lang_name: "English", resource_type: "tn", resource_type_name: "Translation Notes", book_code: "jud", lang_direction: "ltr", dir: "en_tn" }], tree: { en_tn: tree(join(content, "en_tn"), "jud") }, usfm: [{ lang_code: "en", lang_name: "English", book_code: "jud", resource_type_name: "Unlocked Literal Bible", source: readFileSync(join(content, "en_ulb/66-JUD.usfm"), "utf8") }] };
const [, first] = await time(() => core.parse(input, signal));
t("core thread: first parse (thread start + wasm load)", first);
const runsParse: number[] = [];
let parsed = await core.parse(input, signal);
for (let i = 0; i < 20; i += 1) runsParse.push((await time(async () => { parsed = await core.parse(input, signal); }))[1]);
t("core thread: parse, median of 20", runsParse.sort((a, b) => a - b)[10] as number);
const assembleIn = { request, books: parsed.books, structure: "proposed_look", bug_compat: "fixed", unresolved: [], findings: parsed.findings };
let assembled = await core.assemble(assembleIn, signal);
const runsAsm: number[] = [];
for (let i = 0; i < 20; i += 1) runsAsm.push((await time(async () => { assembled = await core.assemble(assembleIn, signal); }))[1]);
t("core thread: assemble, median of 20", runsAsm.sort((a, b) => a - b)[10] as number);

const job: DocxJob = { version: 2, format: "docx", request_id: "bench", job_id: "bench", request_key: "bench", parts: assembled.parts.map((p) => ({ ...p, content: String(p["content"] ?? "") })), layout_for_print: false, title1: parsed.title1, title2: parsed.title2, book: "jud", output_path: join(tmp, "out.docx") };
const [json, ser] = await time(() => JSON.stringify(DocxJob.parse(job)));
t(`DocxJob serialise + zod parse (${(json.length / 1024).toFixed(0)} KB)`, ser);
writeFileSync(join(tmp, "job.json"), json);
console.log(`parts: ${job.parts.length}; parts JSON ${(JSON.stringify(assembled.parts).length / 1024).toFixed(0)} KB; books JSON across the seam ${(JSON.stringify(parsed.books).length / 1024).toFixed(0)} KB`);

const pythons = process.argv.slice(2).length ? process.argv.slice(2) : ["python3"];
for (const py of pythons) {
  const ver = execFileSync(py, ["-c", "import platform;print(platform.python_version())"], { encoding: "utf8" }).trim();
  for (const bytecode of [false, true]) {
    const env = { ...process.env, ...(bytecode ? {} : { PYTHONDONTWRITEBYTECODE: "1" }) };
    if (bytecode) spawnSync(py, [join(root, "python/docx_worker.py"), join(tmp, "job.json")], { env }); // warm the .pyc files
    const walls: number[] = [];
    const inner: number[] = [];
    const rss: number[] = [];
    for (let i = 0; i < 15; i += 1) {
      const s = performance.now();
      const r = spawnSync(py, [join(root, "python/docx_worker.py"), join(tmp, "job.json")], { env, encoding: "utf8" });
      walls.push(performance.now() - s);
      const o = JSON.parse(r.stdout) as { ms: number; max_rss_kb: number };
      inner.push(o.ms);
      rss.push(o.max_rss_kb);
    }
    const med = (a: number[]) => [...a].sort((x, y) => x - y)[Math.floor(a.length / 2)] as number;
    t(`python ${ver} (${py.includes("venv") ? "venv" : "system"}) ${bytecode ? "with" : "no  "} .pyc: wall median of 15`, med(walls));
    t(`   of which in run() (generation + write)`, med(inner));
    console.log(`   peak RSS of the python process: ${med(rss)} kB`);
  }
  const bare: number[] = [];
  for (let i = 0; i < 15; i += 1) {
    const s = performance.now();
    spawnSync(py, ["-c", "pass"]);
    bare.push(performance.now() - s);
  }
  t(`python ${ver} bare interpreter start, median of 15`, [...bare].sort((a, b) => a - b)[7] as number);
}
await validateDocx(join(tmp, "out.docx"));
console.log("output validated by the shell's own check");
await thread.close();
clearInterval(keepAlive);
