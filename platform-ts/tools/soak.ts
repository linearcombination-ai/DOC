/**
 * The soak: mixed requests against a running stack (API process, two worker processes with persistent docx pools, Redis, the data
 * API mock and local git remotes), memory of every process sampled, then drained and checked for leftovers.
 *
 *   node tools/soak.ts <minutes=30> <out dir> [requests per second=3]
 *
 * Needs REDIS_SERVER_BIN, a built core/pkg and core/pkg-api. PYTHON_BIN should be a Python with WeasyPrint for the PDF share of the
 * mix (the DOC venv's). Prints a summary and writes samples.csv and summary.json to the out directory.
 */
import { execFileSync, spawn, type ChildProcess } from "node:child_process";
import { mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { createStack, judeRequest, PROTO } from "../test/support/stack.ts";

const minutes = Number(process.argv[2] ?? 30);
const outDir = resolve(process.argv[3] ?? "soak-results");
const rate = Number(process.argv[4] ?? 3);
mkdirSync(outDir, { recursive: true });
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const rssMb = (pid: number): number => {
  try {
    return Number(/VmRSS:\s+(\d+)/.exec(readFileSync(`/proc/${pid}/status`, "utf8"))?.[1] ?? 0) / 1024;
  } catch {
    return 0;
  }
};
const fds = (pid: number): number => {
  try {
    return readdirSync(`/proc/${pid}/fd`).length;
  } catch {
    return 0;
  }
};
const children = (pid: number): number[] => {
  try {
    return execFileSync("pgrep", ["-P", String(pid)], { encoding: "utf8" }).split("\n").filter(Boolean).map(Number);
  } catch {
    return [];
  }
};
const descendantsRss = (pid: number): { mb: number; n: number } => {
  let mb = 0;
  let n = 0;
  for (const c of children(pid)) {
    const d = descendantsRss(c);
    mb += rssMb(c) + d.mb;
    n += 1 + d.n;
  }
  return { mb, n };
};

const python = process.env["PYTHON_BIN"] ?? "python3";
const s = await createStack({ env: { OUTPUT_TTL_MINUTES: "1", DOCX_MODE: "persistent", DOCX_POOL_SIZE: "2", WORKER_CONCURRENCY: "4", PYTHON_BIN: python, STAGE_DOCX_DEADLINE_MS: "120000", HEARTBEAT_INTERVAL_MS: "2000", LOG_LEVEL: "warn", JOB_BACKOFF_BASE_MS: "500", JOB_BACKOFF_CAP_MS: "5000", LOCK_DURATION_MS: "30000", STALLED_INTERVAL_MS: "15000" } });
const port = 29_000 + (process.pid % 500);
const env = { ...process.env, ...s.env, PORT: String(port), LOG_LEVEL: "warn" };
const api = spawn(process.execPath, [join(PROTO, "src/main.ts")], { env, cwd: PROTO, stdio: ["ignore", "inherit", "inherit"] });
const workers: ChildProcess[] = [s.spawnWorker({ LOG_LEVEL: "warn" }), s.spawnWorker({ LOG_LEVEL: "warn" })];
const base = `http://127.0.0.1:${port}`;
for (let i = 0; i < 200; i += 1) {
  try {
    const h = await fetch(`${base}/health/status`).then((r) => r.json() as Promise<[{ workers?: unknown[] }, number]>);
    if ((h[0].workers?.length ?? 0) >= 2) break;
  } catch {
    // not up yet
  }
  await sleep(100);
}

const types = ["ulb", "tn", "tq", "tw"];
const perms = (xs: string[]): string[][] => (xs.length <= 1 ? [xs] : xs.flatMap((x, i) => perms([...xs.slice(0, i), ...xs.slice(i + 1)]).map((p) => [x, ...p])));
const orders = perms(types);
const randomRequest = (extra: Record<string, unknown> = {}): Record<string, unknown> => {
  const order = orders[Math.floor(Math.random() * orders.length)] ?? types;
  return judeRequest({ layout_for_print: Math.random() < 0.5, resource_requests: order.map((t) => ({ lang_code: "en", resource_type: t, book_code: "jud" })), ...extra });
};
const post = (path: string, body: unknown) => fetch(`${base}${path}`, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(body) });

interface Tally { sent: number; ok: number; failed: number; rejected: number; latencies: number[]; failures: string[] }
const tally: Record<string, Tally> = {};
const t = (k: string): Tally => (tally[k] ??= { sent: 0, ok: 0, failed: 0, rejected: 0, latencies: [], failures: [] });

async function job(kind: string, path: string, body: Record<string, unknown>): Promise<void> {
  const m = t(kind);
  m.sent += 1;
  const started = performance.now();
  try {
    const res = await post(path, body);
    const json = (await res.json()) as { task_id?: string };
    if (!res.ok || !json.task_id) {
      m.failed += 1;
      m.failures.push(`POST ${res.status}`);
      return;
    }
    for (let i = 0; i < 1200; i += 1) {
      const st = (await (await fetch(`${base}/task_status/${json.task_id}`)).json()) as { state: string; result?: unknown };
      if (st.state === "SUCCESS") {
        m.ok += 1;
        m.latencies.push(performance.now() - started);
        return;
      }
      if (st.state === "FAILURE") {
        m.failed += 1;
        m.failures.push(JSON.stringify(st.result).slice(0, 120));
        return;
      }
      await sleep(150);
    }
    m.failed += 1;
    m.failures.push("timeout");
  } catch (e) {
    m.failed += 1;
    m.failures.push(e instanceof Error ? e.message : String(e));
  }
}

async function oneRequest(): Promise<void> {
  const r = Math.random();
  if (r < 0.4) return job("docx", "/documents_docx", randomRequest());
  if (r < 0.58) return job("html", "/documents", randomRequest({ generate_docx: false }));
  if (r < 0.63) return job("pdf", "/documents", randomRequest({ generate_docx: false, generate_pdf: true }));
  if (r < 0.73) {
    const body = randomRequest({ resource_requests: [{ lang_code: "en", resource_type: "ulb", book_code: "jud" }, { lang_code: "en", resource_type: "tn", book_code: "jud" }] });
    return Promise.all([job("duplicate", "/documents_docx", body), job("duplicate", "/documents_docx", body), job("duplicate", "/documents_docx", body)]).then(() => undefined);
  }
  if (r < 0.78) {
    const m = t("invalid");
    m.sent += 1;
    const res = await post("/documents_docx", { resource_requests: "nope" });
    if (res.status === 422) m.rejected += 1;
    else m.failed += 1;
    return;
  }
  if (r < 0.83) {
    const m = t("unknown_task");
    m.sent += 1;
    const st = (await (await fetch(`${base}/task_status/does-not-exist-${Math.random()}`)).json()) as { state: string };
    if (st.state === "PENDING") m.ok += 1;
    else m.failed += 1;
    return;
  }
  if (r < 0.93) {
    const m = t("lookup");
    m.sent += 1;
    const res = await fetch(`${base}${Math.random() < 0.5 ? "/language_codes_and_names" : "/book_codes_for_lang/en"}`);
    await res.text();
    if (res.ok) m.ok += 1;
    else m.failed += 1;
    return;
  }
  const m = t("scrape");
  m.sent += 1;
  const res = await fetch(`${base}${Math.random() < 0.5 ? "/metrics" : "/health/status"}`);
  await res.text();
  if (res.ok) m.ok += 1;
  else m.failed += 1;
}

const redisPid = s.redis.pid;
const csv = ["t_s,load1,api_mb,w1_mb,w1_py_mb,w1_py_n,w2_mb,w2_py_mb,w2_py_n,redis_mb,api_fds,w1_fds,queue_waiting,queue_active,inflight"];
const samples: Record<string, number>[] = [];
const started = Date.now();
let inflight = 0;
const sampler = setInterval(() => {
  const p = [workers[0]?.pid ?? 0, workers[1]?.pid ?? 0];
  const d = p.map((x) => descendantsRss(x));
  const row = { t: (Date.now() - started) / 1000, load1: Number(readFileSync("/proc/loadavg", "utf8").split(" ")[0]), api: rssMb(api.pid ?? 0), w1: rssMb(p[0] ?? 0), w1py: d[0]?.mb ?? 0, w1n: d[0]?.n ?? 0, w2: rssMb(p[1] ?? 0), w2py: d[1]?.mb ?? 0, w2n: d[1]?.n ?? 0, redis: rssMb(redisPid), apiFds: fds(api.pid ?? 0), w1Fds: fds(p[0] ?? 0) };
  samples.push(row);
  csv.push(`${row.t.toFixed(0)},${row.load1},${row.api.toFixed(1)},${row.w1.toFixed(1)},${row.w1py.toFixed(1)},${row.w1n},${row.w2.toFixed(1)},${row.w2py.toFixed(1)},${row.w2n},${row.redis.toFixed(1)},${row.apiFds},${row.w1Fds},,,${inflight}`);
  writeFileSync(join(outDir, "samples.csv"), csv.join("\n") + "\n");
}, 10_000);

console.log(`soak: ${minutes} min at ~${rate} requests/s, out ${outDir}, python ${python}, host load at start ${readFileSync("/proc/loadavg", "utf8").trim()}`);
const until = Date.now() + minutes * 60_000;
const pending = new Set<Promise<void>>();
while (Date.now() < until) {
  if (inflight < 24) {
    inflight += 1;
    const p = oneRequest().catch(() => undefined).finally(() => {
      inflight -= 1;
      pending.delete(p);
    });
    pending.add(p);
  }
  await sleep(1000 / rate);
}
console.log("load stopped, draining in-flight requests...");
await Promise.race([Promise.all(pending), sleep(180_000)]);
clearInterval(sampler);

// leftovers after a graceful stop of everything
const health = (await (await fetch(`${base}/health/status`)).json()) as [Record<string, unknown>, number];
const metricsText = await (await fetch(`${base}/metrics`)).text();
for (const w of workers) w.kill("SIGTERM");
await Promise.all(workers.map((w) => new Promise((r) => (w.exitCode !== null ? r(0) : w.once("exit", r)))));
api.kill("SIGTERM");
await new Promise((r) => (api.exitCode !== null ? r(0) : api.once("exit", r)));
const strays = (() => {
  try {
    return execFileSync("pgrep", ["-f", `docx_server.py`], { encoding: "utf8" }).split("\n").filter(Boolean).length;
  } catch {
    return 0;
  }
})();
const scratch = (() => {
  try {
    return readdirSync(join(s.dirs.work, "jobs")).length;
  } catch {
    return 0;
  }
})();

/** Least squares slope in MB per minute over the samples after the warm-up. */
const slope = (key: string, warmupS = 300): number => {
  const xs = samples.filter((r) => (r["t"] ?? 0) > warmupS);
  if (xs.length < 5) return Number.NaN;
  const n = xs.length;
  const mx = xs.reduce((a, r) => a + (r["t"] ?? 0), 0) / n;
  const my = xs.reduce((a, r) => a + (r[key] ?? 0), 0) / n;
  const num = xs.reduce((a, r) => a + ((r["t"] ?? 0) - mx) * ((r[key] ?? 0) - my), 0);
  const den = xs.reduce((a, r) => a + ((r["t"] ?? 0) - mx) ** 2, 0);
  return (num / den) * 60;
};
const pct = (xs: number[], q: number): number => [...xs].sort((a, b) => a - b)[Math.min(xs.length - 1, Math.floor(xs.length * q))] ?? 0;
const mem = (key: string): { first: number; last: number; max: number; slopeMbPerMin: number } => {
  const vals = samples.map((r) => r[key] ?? 0);
  return { first: vals[Math.min(vals.length - 1, 30)] ?? 0, last: vals.at(-1) ?? 0, max: Math.max(...vals), slopeMbPerMin: Number(slope(key).toFixed(3)) };
};
const summary = {
  minutes,
  hostLoad: { start: samples[0]?.["load1"], max: Math.max(...samples.map((r) => r["load1"] ?? 0)) },
  requests: Object.fromEntries(Object.entries(tally).map(([k, v]) => [k, { sent: v.sent, ok: v.ok, failed: v.failed, rejected: v.rejected, p50: Math.round(pct(v.latencies, 0.5)), p95: Math.round(pct(v.latencies, 0.95)), max: Math.round(Math.max(0, ...v.latencies)), failureSamples: [...new Set(v.failures)].slice(0, 5) }])),
  memoryMb: { api: mem("api"), worker1: mem("w1"), worker1Python: mem("w1py"), worker2: mem("w2"), worker2Python: mem("w2py"), redis: mem("redis") },
  fds: { apiFirst: samples[30]?.["apiFds"], apiLast: samples.at(-1)?.["apiFds"], worker1First: samples[30]?.["w1Fds"], worker1Last: samples.at(-1)?.["w1Fds"] },
  leftovers: { pythonProcessesAfterStop: strays, scratchDirectories: scratch },
  queueAtEnd: (health[0] as { queue?: unknown }).queue,
  healthStatusAtEnd: health[0]["status"],
  metricsLines: metricsText.split("\n").length,
  dropped: /doc_metrics_dropped_total (\d+)/.exec(metricsText)?.[1],
};
writeFileSync(join(outDir, "summary.json"), JSON.stringify(summary, null, 2));
writeFileSync(join(outDir, "metrics-at-end.txt"), metricsText);
console.log(JSON.stringify(summary, null, 2));
await s.close();
process.exit(0);
