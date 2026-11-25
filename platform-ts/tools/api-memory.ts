/**
 * P6 item X: what does it cost the API process to load the wasm core? Starts the API with the light module (core/pkg-api, no
 * interleaver, markdown or USFM) and with the full module (core/pkg, what P5 loaded), against a 21,000-row catalog, and samples
 * resident memory from /proc: right after start, after the first catalog normalisation, and after 300 mixed requests.
 *
 *   node tools/api-memory.ts [runs=3]            (needs REDIS_SERVER_BIN, a built core/pkg and core/pkg-api)
 */
import { spawn } from "node:child_process";
import { readFileSync, mkdtempSync, mkdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { startRedis } from "../test/support/stack.ts";
import { startMockDataApi } from "../test/support/mockDataApi.ts";
import { envelope } from "../test/support/snapshotFixture.ts";
import { bigCatalogRows } from "../test/support/bigCatalog.ts";

const root = resolve(import.meta.dirname, "..");
const runs = Number(process.argv[2] ?? 3);
const rss = (pid: number): number => Number(/VmRSS:\s+(\d+)/.exec(readFileSync(`/proc/${pid}/status`, "utf8"))?.[1] ?? 0) / 1024;
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const api = await startMockDataApi({ primary: envelope(bigCatalogRows()), nonPrimary: envelope([]) });
const redis = await startRedis();
const base = mkdtempSync(join(process.env["TMPDIR"] ?? tmpdir(), "doc-apimem-"));
for (const d of ["out", "work", "assets"]) mkdirSync(join(base, d));

async function measure(label: string, wasmApiDir: string, port: number): Promise<number[]> {
  const child = spawn(process.execPath, [join(root, "src/main.ts")], {
    cwd: root,
    env: { ...process.env, PORT: String(port), DATA_API_URL: api.url, DOC_MODE: "fixed", LOG_LEVEL: "warn", REDIS_URL: redis.url, QUEUE_PREFIX: `m${port}`, DOCUMENT_OUTPUT_DIR: join(base, "out"), WORKING_DIR: join(base, "work"), RESOURCE_ASSETS_DIR: join(base, "assets"), WASM_API_PKG_DIR: wasmApiDir, WASM_PKG_DIR: join(root, "core/pkg"), DOCX_SERVER_SCRIPT: join(root, "python/docx_server.py"), DOCX_WORKER_SCRIPT: join(root, "python/docx_worker.py") },
    stdio: ["ignore", "ignore", "inherit"],
  });
  const url = `http://127.0.0.1:${port}`;
  for (let i = 0; i < 100; i += 1) {
    try {
      if ((await fetch(`${url}/health/status`)).ok) break;
    } catch {
      await sleep(100);
    }
  }
  await sleep(500);
  const started = rss(child.pid as number);
  await fetch(`${url}/language_codes_and_names`).then((r) => r.text());
  const normalised = rss(child.pid as number);
  const body = JSON.stringify({ email_address: null, assembly_strategy_kind: "lbo", layout_for_print: false, resource_requests: [{ lang_code: "en", resource_type: "ulb", book_code: "jud" }], generate_pdf: false, generate_epub: false, generate_docx: true, limit_words: true, document_request_source: "ui" });
  for (let i = 0; i < 300; i += 1) {
    const which = i % 3;
    if (which === 0) await fetch(`${url}/language_codes_and_names`).then((r) => r.text());
    else if (which === 1) await fetch(`${url}/health/status`).then((r) => r.text());
    else await fetch(`${url}/documents_docx`, { method: "POST", headers: { "content-type": "application/json" }, body }).then((r) => r.text());
  }
  await sleep(500);
  const after = rss(child.pid as number);
  child.kill("SIGTERM");
  await new Promise((r) => child.once("exit", r));
  console.log(`${label.padEnd(34)} rss MB: at start ${started.toFixed(0)}, after catalog ${normalised.toFixed(0)}, after 300 requests ${after.toFixed(0)}`);
  return [started, normalised, after];
}
const all: Record<string, number[][]> = { light: [], full: [] };
let port = 19_100;
for (let i = 0; i < runs; i += 1) {
  all["light"]?.push(await measure(`light module (pkg-api, ${i + 1})`, join(root, "core/pkg-api"), (port += 1)));
  all["full"]?.push(await measure(`full module (pkg, ${i + 1})`, join(root, "core/pkg"), (port += 1)));
}
const med = (xs: number[]): number => [...xs].sort((a, b) => a - b)[Math.floor(xs.length / 2)] ?? 0;
for (const [k, v] of Object.entries(all)) console.log(`median ${k.padEnd(6)} start ${med(v.map((r) => r[0] ?? 0)).toFixed(0)} MB, after catalog ${med(v.map((r) => r[1] ?? 0)).toFixed(0)} MB, after requests ${med(v.map((r) => r[2] ?? 0)).toFixed(0)} MB`);
await api.close();
await redis.kill();
process.exit(0);
