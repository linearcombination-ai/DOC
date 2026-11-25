/**
 * The test stack for the job path: a real Redis (the binary built into the worktree volume), the mock data API, local git
 * "remotes" holding a real (small) slice of the English content, and either an in-process worker or a worker process.
 */
import { type ChildProcess, spawn } from "node:child_process";
import { mkdirSync, mkdtempSync, readdirSync, readFileSync, statSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { Redis } from "ioredis";
import { QueueEvents } from "bullmq";
import { loadConfig } from "../../src/config.ts";
import { loadJobConfig, type JobConfig } from "../../src/jobs/config.ts";
import { startWorker, type RunningWorker } from "../../src/jobs/worker.ts";
import { buildApp } from "../../src/main.ts";
import { createLogger, type Log } from "../../src/util/log.ts";
import { createFixtures, HOST, makeFlakyGit, type Fixtures } from "./gitFixtures.ts";
import { startMockDataApi, type MockDataApi } from "./mockDataApi.ts";
import { envelope } from "./snapshotFixture.ts";

const here = dirname(fileURLToPath(import.meta.url));
export const PROTO = resolve(here, "../..");
const TMP = process.env["TMPDIR"] ?? tmpdir();

export interface RedisHandle {
  port: number;
  url: string;
  pid: number;
  dir: string;
  stop(): Promise<void>;
  kill(): Promise<void>;
  /** Start again on the same port and data directory (AOF replays). */
  restart(): Promise<void>;
}

let nextPort = 26_000 + (process.pid % 2_000);
const freePort = (): number => nextPort++;

export async function startRedis(opts: { aof?: boolean; extraArgs?: string[] } = {}): Promise<RedisHandle> {
  const bin = process.env["REDIS_SERVER_BIN"] ?? "redis-server";
  const dir = mkdtempSync(join(TMP, "doc-redis-"));
  const port = freePort();
  let child: ChildProcess | undefined;
  const args = ["--port", String(port), "--bind", "127.0.0.1", "--dir", dir, "--save", "", "--appendonly", opts.aof === false ? "no" : "yes", "--appendfsync", "everysec", "--loglevel", "warning", ...(opts.extraArgs ?? [])];
  const handle: RedisHandle = {
    port,
    url: `redis://127.0.0.1:${port}/0`,
    pid: 0,
    dir,
    async stop() {
      if (!child) return;
      const c = child;
      child = undefined;
      c.kill("SIGTERM");
      await new Promise<void>((r) => c.once("exit", () => r()));
    },
    async kill() {
      if (!child) return;
      const c = child;
      child = undefined;
      c.kill("SIGKILL");
      await new Promise<void>((r) => c.once("exit", () => r()));
    },
    async restart() {
      await start();
    },
  };
  async function start(): Promise<void> {
    child = spawn(bin, args, { stdio: "ignore" });
    handle.pid = child.pid ?? 0;
    for (let i = 0; i < 100; i += 1) {
      const probe = new Redis(handle.url, { lazyConnect: true, maxRetriesPerRequest: 0, retryStrategy: () => null });
      probe.on("error", () => undefined);
      try {
        await probe.connect();
        await probe.ping();
        probe.disconnect();
        return;
      } catch {
        probe.disconnect();
        await new Promise((r) => setTimeout(r, 50));
      }
    }
    throw new Error("redis did not start");
  }
  await start();
  return handle;
}

/** A document request for one short interleaved resource pair: English ULB with Translation Notes for Jude. */
export function judeRequest(overrides: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    email_address: null,
    assembly_strategy_kind: "lbo",
    layout_for_print: false,
    resource_requests: [
      { lang_code: "en", resource_type: "ulb", book_code: "jud" },
      { lang_code: "en", resource_type: "tn", book_code: "jud" },
    ],
    generate_pdf: false,
    generate_epub: false,
    generate_docx: true,
    limit_words: true,
    document_request_source: "ui",
    ...overrides,
  };
}

function readTree(root: string, rel = ""): Record<string, string> {
  const out: Record<string, string> = {};
  for (const name of readdirSync(join(root, rel))) {
    const p = rel ? `${rel}/${name}` : name;
    if (statSync(join(root, p)).isDirectory()) Object.assign(out, readTree(root, p));
    else out[p] = readFileSync(join(root, p), "utf8");
  }
  return out;
}

export interface Stack {
  redis: RedisHandle;
  api: MockDataApi;
  fx: Fixtures;
  flaky: ReturnType<typeof makeFlakyGit>;
  dirs: { out: string; work: string; assets: string; root: string };
  env: Record<string, string>;
  jobConfig: JobConfig;
  app: Awaited<ReturnType<typeof buildApp>>;
  worker?: RunningWorker;
  /** Start an in-process worker (call again with other env to simulate a second worker). */
  startWorker(extra?: Record<string, string>): Promise<RunningWorker>;
  /** Spawn a worker process (`node src/worker-main.ts`) with this stack's environment. */
  spawnWorker(extra?: Record<string, string>): ChildProcess;
  post(body?: Record<string, unknown>, path?: string): Promise<{ status: number; json: Record<string, unknown>; headers: Headers }>;
  status(id: string): Promise<Record<string, unknown>>;
  waitFor(id: string, states?: string[], timeoutMs?: number): Promise<{ trace: string[]; last: Record<string, unknown> }>;
  /** Every progress update the worker publishes, in order, per job id (the source of truth for the state sequence; polling can miss fast stages). */
  states: Map<string, string[]>;
  /** Structured log lines (JSON) every in-process component wrote. */
  logs(): Record<string, unknown>[];
  close(): Promise<void>;
}

export async function createStack(opts: { env?: Record<string, string>; redis?: RedisHandle; log?: boolean } = {}): Promise<Stack> {
  const lines: string[] = [];
  const redis = opts.redis ?? (await startRedis());
  const fx = createFixtures();
  const content = join(PROTO, "test/fixtures/content");
  fx.makeRepo("WycliffeAssociates", "en_ulb", readTree(join(content, "en_ulb")));
  fx.makeRepo("WycliffeAssociates", "en_tn", readTree(join(content, "en_tn")));
  fx.makeRepo("WycliffeAssociates", "en_tq", readTree(join(content, "en_tq")));
  fx.makeRepo("WycliffeAssociates", "en_tw", readTree(join(content, "en_tw")));
  const flaky = makeFlakyGit(join(fx.root, "flaky"));
  const EN = { english_name: "English", ietf_code: "en", national_name: "English", direction: "ltr" };
  const row = (n: string, t: string) => ({ repo_url: `${HOST}WycliffeAssociates/${n}`, content: { resource_type: t, language: EN } });
  const api = await startMockDataApi({ primary: envelope([row("en_ulb", "ulb"), row("en_tn", "tn"), row("en_tq", "tq"), row("en_tw", "tw")]), nonPrimary: envelope([]) });
  const root = mkdtempSync(join(TMP, "doc-p5-"));
  const dirs = { root, out: join(root, "out"), work: join(root, "work"), assets: join(root, "assets") };
  for (const d of [dirs.out, dirs.work, dirs.assets]) mkdirSync(d, { recursive: true });
  const env: Record<string, string> = {
    DATA_API_URL: api.url, RESOURCE_ASSETS_DIR: dirs.assets, GIT_BIN: flaky.bin, DOC_MODE: "fixed", LOG_LEVEL: "info",
    DATA_API_TIMEOUT_MS: "1000", DATA_API_MAX_ATTEMPTS: "3", DATA_API_BACKOFF_BASE_MS: "5", DATA_API_BACKOFF_CAP_MS: "20", DATA_API_DEADLINE_MS: "8000",
    GIT_MAX_ATTEMPTS: "3", GIT_BACKOFF_BASE_MS: "5", GIT_BACKOFF_CAP_MS: "20", GIT_CLONE_TIMEOUT_MS: "20000",
    REDIS_URL: redis.url, QUEUE_PREFIX: `t${process.pid}x${Math.floor(Math.random() * 1e6)}`, DOCUMENT_OUTPUT_DIR: dirs.out, WORKING_DIR: dirs.work,
    JOB_BACKOFF_BASE_MS: "50", JOB_BACKOFF_CAP_MS: "400", LOCK_DURATION_MS: "2000", STALLED_INTERVAL_MS: "500", WORKER_CONCURRENCY: "2",
    PYTHON_BIN: process.env["PYTHON_BIN"] ?? "python3", DOCX_WORKER_SCRIPT: join(PROTO, "python/docx_worker.py"), DOCX_SERVER_SCRIPT: join(PROTO, "python/docx_server.py"), DOCX_MODE: process.env["DOCX_MODE"] ?? "process", WASM_PKG_DIR: join(PROTO, "core/pkg"), WASM_API_PKG_DIR: join(PROTO, "core/pkg-api"),
    GIT_CONFIG_GLOBAL: fx.gitConfig, GIT_CONFIG_NOSYSTEM: "1",
    ...opts.env,
  };
  Object.assign(process.env, { GIT_CONFIG_GLOBAL: fx.gitConfig, GIT_CONFIG_NOSYSTEM: "1" });
  const config = loadConfig(env);
  const jobConfig = loadJobConfig(env);
  const log: Log = createLogger(opts.log ? "info" : "info", { write: (chunk: string) => { lines.push(chunk); if (opts.log) process.stderr.write(chunk); return true; } } as unknown as NodeJS.WritableStream);
  const app = await buildApp(config, log, undefined, jobConfig);
  const states = new Map<string, string[]>();
  const eventsConn = new Redis(redis.url, { maxRetriesPerRequest: null });
  eventsConn.on("error", () => undefined);
  const events = new QueueEvents(jobConfig.QUEUE_NAME, { connection: eventsConn, prefix: jobConfig.QUEUE_PREFIX });
  events.on("error", () => undefined);
  events.on("progress", ({ jobId, data }) => {
    const state = (data as { state?: string }).state;
    if (state) states.set(jobId, [...(states.get(jobId) ?? []), state]);
  });
  await events.waitUntilReady();
  const children: ChildProcess[] = [];
  const workers: RunningWorker[] = [];
  const builds: Awaited<ReturnType<typeof buildApp>>[] = [app];
  const stack: Stack = {
    redis, api, fx, flaky, dirs, env, jobConfig, app, states,
    logs: () => lines.flatMap((l) => l.split("\n").filter(Boolean).map((x) => JSON.parse(x) as Record<string, unknown>)),
    async startWorker(extra = {}) {
      const merged = { ...env, ...extra };
      const built = extra && Object.keys(extra).length > 0 ? await buildApp(loadConfig(merged), log, undefined, loadJobConfig(merged)) : app;
      if (built !== app) builds.push(built);
      const docx = built.docx;
      if (!docx) throw new Error("no docx runner");
      await docx.warm();
      const w = await startWorker({ config: built.config, jobs: loadJobConfig(merged), log, lookup: built.lookup, acquirer: built.acquirer, core: built.jobCore, docx, metrics: built.metrics, ...(built.jobs ? { redis: built.jobs.service.connection } : {}) }, { metrics: built.metrics, ...(built.telemetry ? { telemetry: built.telemetry } : {}) });
      workers.push(w);
      stack.worker = w;
      return w;
    },
    spawnWorker(extra = {}) {
      const child = spawn(process.execPath, [join(PROTO, "src/worker-main.ts")], { env: { ...process.env, ...env, ...extra, LOG_LEVEL: opts.log ? "info" : "warn" }, stdio: ["ignore", "inherit", "inherit"], cwd: PROTO });
      children.push(child);
      return child;
    },
    async post(body = judeRequest(), path = "/documents_docx") {
      const res = await app.app.request(path, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(body) });
      return { status: res.status, json: (await res.json()) as Record<string, unknown>, headers: res.headers };
    },
    async status(id) {
      return (await (await app.app.request(`/task_status/${id}`)).json()) as Record<string, unknown>;
    },
    async waitFor(id, terminal = ["SUCCESS", "FAILURE"], timeoutMs = 60_000) {
      const trace: string[] = [];
      const until = Date.now() + timeoutMs;
      for (;;) {
        const s = await stack.status(id);
        const state = String(s["state"]);
        if (trace.at(-1) !== state) trace.push(state);
        if (terminal.includes(state)) return { trace, last: s };
        if (Date.now() > until) throw new Error(`timed out waiting for ${id}; trace ${JSON.stringify(trace)}`);
        await new Promise((r) => setTimeout(r, 15));
      }
    },
    async close() {
      // every step is bounded: a teardown that hangs on a connection to a server the test killed must not hide the real result
      const bounded = async (label: string, work: Promise<unknown>, ms = 8_000): Promise<void> => {
        let timer: NodeJS.Timeout | undefined;
        const timeout = new Promise<void>((resolve) => {
          timer = setTimeout(() => {
            process.stderr.write(`TEARDOWN step "${label}" did not finish in ${ms} ms; continuing\n`);
            resolve();
          }, ms);
        });
        await Promise.race([work.then(() => undefined, () => undefined), timeout]);
        clearTimeout(timer);
      };
      for (const c of children) if (c.exitCode === null) c.kill("SIGKILL");
      for (const w of workers) await bounded("worker.close", w.close(true));
      await bounded("events.close", events.close());
      eventsConn.disconnect();
      for (const b of builds) {
        await bounded("docx.close", b.docx?.close() ?? Promise.resolve());
        await bounded("thread.close", b.thread.close());
        if (b !== app) await bounded("service.close", b.jobs?.service.close() ?? Promise.resolve());
      }
      await bounded("app service.close", app.jobs?.service.close() ?? Promise.resolve());
      await bounded("mock api close", api.close());
      if (!opts.redis) await bounded("redis.kill", redis.kill());
      const { execFileSync } = await import("node:child_process");
      execFileSync("rm", ["-rf", fx.root, root, ...(opts.redis ? [] : [redis.dir])]);
    },
  };
  return stack;
}
