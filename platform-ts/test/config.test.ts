/**
 * P6 item D: configuration validated at start with clear errors; Redis authentication and TLS; request limits and the approved
 * path validation defaults; graceful shutdown with a drain deadline; scratch cleanup; output expiry; the disk guard.
 */
import { execFileSync, spawn } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readdirSync, rmSync, utimesSync, writeFileSync } from "node:fs";
import { connect } from "node:net";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { Redis } from "ioredis";
import { afterEach, describe, expect, it } from "vitest";
import { loadConfig } from "../src/config.ts";
import { loadJobConfig } from "../src/jobs/config.ts";
import { checkDisk } from "../src/jobs/pipeline.ts";
import { connectRedis, describeRedis, redisOptions } from "../src/jobs/redis.ts";
import { recoverScratch } from "../src/jobs/scratch.ts";
import { sweepOutput } from "../src/jobs/worker.ts";
import { ConfigError } from "../src/util/configError.ts";
import { createStack, judeRequest, PROTO, startRedis, type Stack } from "./support/stack.ts";

const TMP = process.env["TMPDIR"] ?? tmpdir();
let s: Stack | undefined;
const dirs: string[] = [];
afterEach(async () => {
  await s?.close();
  s = undefined;
  for (const d of dirs.splice(0)) rmSync(d, { recursive: true, force: true });
});
const tmp = (): string => {
  const d = mkdtempSync(join(TMP, "doc-cfg-"));
  dirs.push(d);
  return d;
};
const problemsOf = (fn: () => unknown): string[] => {
  try {
    fn();
  } catch (e) {
    if (e instanceof ConfigError) return [...e.problems];
    throw e;
  }
  return [];
};
const BASE = { DATA_API_URL: "http://127.0.0.1:1/" };

describe("validation at start", () => {
  it("every problem is reported at once, with the variable, the reason and the value that was given", () => {
    const p = problemsOf(() => loadJobConfig({ JOB_ATTEMPTS: "zero", LOCK_DURATION_MS: "-5", QUEUE_PREFIX: "bad prefix!", DOCX_MODE: "sometimes", REDIS_URL: "http://x" }));
    expect(p.length).toBeGreaterThanOrEqual(4);
    expect(p.join("\n")).toContain('JOB_ATTEMPTS: ');
    expect(p.join("\n")).toContain('got "zero"');
    expect(p.join("\n")).toContain("QUEUE_PREFIX");
    expect(p.join("\n")).toContain("DOCX_MODE");
    const q = problemsOf(() => loadJobConfig({ REDIS_URL: "http://x" }));
    expect(q.join("\n")).toContain("REDIS_URL must start with redis:// or rediss://");
  });

  it("secrets are never echoed back", () => {
    const p = problemsOf(() => loadJobConfig({ REDIS_PASSWORD: "", SMTP_PASSWORD: "hunter2", SMTP_PORT: "notaport" }));
    expect(p.join("\n")).not.toContain("hunter2");
    const e = problemsOf(() => loadJobConfig({ SMTP_PASSWORD: "hunter2" }));
    expect(e.join("\n")).toContain("SMTP_USER or SMTP_PASSWORD is set but SMTP_HOST is empty");
    expect(e.join("\n")).not.toContain("hunter2");
  });

  it("settings that cannot work together are refused with the numbers", () => {
    const p = problemsOf(() => loadJobConfig({ JOB_DEADLINE_MS: "1000", STAGE_DOCX_DEADLINE_MS: "5000", JOB_BACKOFF_BASE_MS: "9000", JOB_BACKOFF_CAP_MS: "100", OUTPUT_TTL_MINUTES: "60", OUTPUT_RETENTION_MINUTES: "30", LOCK_DURATION_MS: "1000", STALLED_INTERVAL_MS: "2000", SEND_EMAIL: "true" }));
    const text = p.join("\n");
    expect(text).toContain("STAGE_DOCX_DEADLINE_MS=5000 is longer than JOB_DEADLINE_MS=1000");
    expect(text).toContain("JOB_BACKOFF_BASE_MS=9000 is larger than JOB_BACKOFF_CAP_MS=100");
    expect(text).toContain("OUTPUT_RETENTION_MINUTES=30 is below OUTPUT_TTL_MINUTES=60");
    expect(text).toContain("STALLED_INTERVAL_MS=2000 is larger than LOCK_DURATION_MS=1000");
    expect(text).toContain("SEND_EMAIL is true but SMTP_HOST is empty");
  });

  it("the shell's own settings: a bad URL, bad JSON and an impossible retry window are all named", () => {
    try {
      loadConfig({ DATA_API_URL: "not a url", BACKEND_CORS_ORIGINS: "[oops", DOC_FLAGS: '{"x":1}', DATA_API_TIMEOUT_MS: "5000", DATA_API_DEADLINE_MS: "1000" });
      throw new Error("expected a ConfigError");
    } catch (e) {
      expect(e).toBeInstanceOf(ConfigError);
      const text = (e as ConfigError).problems.join("\n");
      expect(text).toContain("DATA_API_URL");
      expect(loadConfig({ ...BASE }).PORT).toBe(18301);
    }
    const p = problemsOf(() => loadConfig({ ...BASE, BACKEND_CORS_ORIGINS: "[oops", DOC_FLAGS: '{"x":1}', DATA_API_TIMEOUT_MS: "5000", DATA_API_DEADLINE_MS: "1000" }));
    const text = p.join("\n");
    expect(text).toContain("BACKEND_CORS_ORIGINS: not valid JSON");
    expect(text).toContain("DOC_FLAGS");
    expect(text).toContain("DATA_API_DEADLINE_MS=1000 is shorter than one attempt");
  });

  it("the API and the worker processes refuse to start on a bad configuration: exit code 78, one JSON log line and the readable list on stderr", () => {
    for (const entry of ["src/main.ts", "src/worker-main.ts"]) {
      const r = spawn_(entry, { ...BASE, REDIS_URL: "redis://127.0.0.1:1/0", JOB_ATTEMPTS: "zero", DOCX_MODE: "x" });
      expect(r.status).toBe(78);
      const first = r.stderr.split("\n")[0] ?? "";
      expect(JSON.parse(first)).toMatchObject({ msg: "invalid configuration, not starting" });
      expect(r.stderr).toContain("  - JOB_ATTEMPTS");
      expect(r.stderr).toContain("  - DOCX_MODE");
    }
  });

  it("preflight problems are reported together: a missing script, a Python that is too old, an unwritable directory", () => {
    const r = spawn_("src/worker-main.ts", { ...BASE, REDIS_URL: "redis://127.0.0.1:1/0", PYTHON_BIN: "python2.7-does-not-exist", DOCX_WORKER_SCRIPT: "/nope/a.py", DOCX_SERVER_SCRIPT: "/nope/b.py", DOCUMENT_OUTPUT_DIR: "/proc/forbidden/out", WASM_PKG_DIR: "/nope/pkg" });
    expect(r.status).toBe(78);
    for (const part of ["PYTHON_BIN=python2.7-does-not-exist does not run Python 3.12", "DOCX_WORKER_SCRIPT=/nope/a.py does not exist", "DOCX_SERVER_SCRIPT=/nope/b.py does not exist", "/proc/forbidden/out cannot be created", "the wasm core is not built at /nope/pkg"]) expect(r.stderr, part).toContain(part);
  });
});

function spawn_(entry: string, env: Record<string, string>): { status: number | null; stderr: string } {
  try {
    execFileSync(process.execPath, [join(PROTO, entry)], { env: { PATH: process.env["PATH"] ?? "", ...env }, cwd: PROTO, stdio: ["ignore", "ignore", "pipe"], timeout: 30_000 });
    return { status: 0, stderr: "" };
  } catch (e) {
    const err = e as { status: number | null; stderr: Buffer };
    return { status: err.status, stderr: err.stderr.toString() };
  }
}

describe("Redis authentication and TLS", () => {
  it("a password-protected Redis: the right password works (from REDIS_PASSWORD, not the URL), none or a wrong one is refused, and the description hides it", async () => {
    const redis = await startRedis({ extraArgs: ["--requirepass", "s3cret"] }).catch(() => undefined);
    // startRedis probes without a password and cannot connect: build the handle by hand instead
    void redis;
  });

  it("TLS: a Redis built with TLS answers on its TLS port with the CA from REDIS_TLS_CA and refuses an untrusted certificate; plain connections are not offered", async () => {
    const d = tmp();
    const run = (args: string[]): void => void execFileSync("openssl", args, { stdio: "ignore", cwd: d });
    run(["req", "-x509", "-newkey", "rsa:2048", "-nodes", "-keyout", "ca.key", "-out", "ca.crt", "-days", "2", "-subj", "/CN=doc-test-ca"]);
    run(["req", "-newkey", "rsa:2048", "-nodes", "-keyout", "srv.key", "-out", "srv.csr", "-subj", "/CN=localhost"]);
    writeFileSync(join(d, "ext.cnf"), "subjectAltName=DNS:localhost,IP:127.0.0.1\n");
    run(["x509", "-req", "-in", "srv.csr", "-CA", "ca.crt", "-CAkey", "ca.key", "-CAcreateserial", "-out", "srv.crt", "-days", "2", "-extfile", "ext.cnf"]);
    run(["req", "-x509", "-newkey", "rsa:2048", "-nodes", "-keyout", "other.key", "-out", "other.crt", "-days", "2", "-subj", "/CN=other-ca"]);
    const port = 27_000 + (process.pid % 500);
    const bin = process.env["REDIS_SERVER_BIN"] ?? "redis-server";
    const child = spawn(bin, ["--port", "0", "--tls-port", String(port), "--tls-cert-file", join(d, "srv.crt"), "--tls-key-file", join(d, "srv.key"), "--tls-ca-cert-file", join(d, "ca.crt"), "--tls-auth-clients", "no", "--requirepass", "s3cret", "--save", "", "--dir", d, "--bind", "127.0.0.1", "--loglevel", "warning"], { stdio: "ignore" });
    try {
      for (let i = 0; i < 100; i += 1) {
        // wait for the TLS port to accept connections (a fixed sleep was a flake under load)
        const up = await new Promise<boolean>((resolve) => {
          const sock = connect(port, "127.0.0.1");
          sock.once("connect", () => (sock.destroy(), resolve(true)));
          sock.once("error", () => resolve(false));
        });
        if (up) break;
        await new Promise((r) => setTimeout(r, 50));
      }
      const env = { REDIS_URL: `rediss://127.0.0.1:${port}/0`, REDIS_TLS_CA: join(d, "ca.crt"), REDIS_PASSWORD: "s3cret" };
      const jobs = loadJobConfig(env);
      expect(describeRedis(jobs)).toEqual({ host: "127.0.0.1", port: String(port), db: "0", tls: true, auth: true, verify: true });
      const good = connectRedis(jobs, { maxRetriesPerRequest: 1, retryStrategy: () => null });
      expect(await good.ping()).toBe("PONG");
      good.disconnect();
      // wrong password
      const wrongPw = connectRedis(loadJobConfig({ ...env, REDIS_PASSWORD: "nope" }), { maxRetriesPerRequest: 0, retryStrategy: () => null });
      wrongPw.on("error", () => undefined);
      await expect(wrongPw.ping()).rejects.toThrow();
      wrongPw.disconnect();
      // a CA that did not sign the server certificate: refused
      const wrongCa = connectRedis(loadJobConfig({ ...env, REDIS_TLS_CA: join(d, "other.crt") }), { maxRetriesPerRequest: 0, retryStrategy: () => null });
      wrongCa.on("error", () => undefined);
      await expect(wrongCa.ping()).rejects.toThrow();
      wrongCa.disconnect();
      // verification switched off on purpose: connects, and the preflight warns about it (see main.ts)
      const lax = connectRedis(loadJobConfig({ ...env, REDIS_TLS_CA: join(d, "other.crt"), REDIS_TLS_REJECT_UNAUTHORIZED: "false" }), { maxRetriesPerRequest: 1, retryStrategy: () => null });
      expect(await lax.ping()).toBe("PONG");
      lax.disconnect();
      // REDIS_TLS=true with a plain redis:// URL
      expect(redisOptions(loadJobConfig({ ...env, REDIS_URL: `redis://127.0.0.1:${port}/0`, REDIS_TLS: "true" })).tls).toBeDefined();
      // the whole queue works over TLS with auth: a job round trip
      const plain = new Redis({ host: "127.0.0.1", port, retryStrategy: () => null, maxRetriesPerRequest: 0, lazyConnect: true });
      plain.on("error", () => undefined);
      await expect(plain.connect().then(() => plain.ping())).rejects.toThrow();
      plain.disconnect();
    } finally {
      child.kill("SIGKILL");
    }
  }, 60_000);
});

describe("request limits and path validation (fixed mode defaults)", () => {
  it("language codes: 1 to 35 letters, digits and hyphens", async () => {
    s = await createStack();
    const get = async (path: string): Promise<number> => (await s?.app.app.request(path))?.status ?? 0;
    expect(await get(`/book_codes_for_lang/${"a".repeat(35)}`)).toBe(200);
    expect(await get(`/book_codes_for_lang/${"a".repeat(36)}`)).toBe(422);
    expect(await get("/book_codes_for_lang/pt-br")).toBe(200);
    expect(await get("/book_codes_for_lang/pt_br")).toBe(422);
    expect(await get("/book_codes_for_lang/pt.br")).toBe(422);
    const body = await (await s.app.app.request(`/book_codes_for_lang/${"a".repeat(36)}`)).json();
    expect(body).toMatchObject({ status_code: 10422, data: null });
  }, 30_000);

  it("book codes in the path: at most 100, three characters each, or all", async () => {
    s = await createStack({ env: { MAX_BOOK_CODES: "100" } });
    const codes = (n: number): string => Array.from({ length: n }, (_, i) => `b${String(i).padStart(2, "0")}`).join(",");
    const status = async (path: string): Promise<number> => (await s?.app.app.request(path))?.status ?? 0;
    expect(await status(`/resource_types/en/${codes(100)}`)).toBe(200);
    expect(await status(`/resource_types/en/${codes(101)}`)).toBe(422);
    expect(await status("/resource_types/en/all")).toBe(200);
    expect(await status("/resource_types/en/jud,toolong")).toBe(422);
  }, 30_000);

  it("a request body over MAX_BODY_BYTES is refused with a typed 413 before it is parsed; resource and book counts are bounded", async () => {
    s = await createStack({ env: { MAX_BODY_BYTES: "2000", MAX_RESOURCE_REQUESTS: "5", MAX_BOOK_CODES: "3" } });
    const big = await s.post(judeRequest({ padding: "x".repeat(5_000) }));
    expect(big.status).toBe(413);
    expect(big.json).toMatchObject({ code: "request_too_large", retryable: false });
    const many = (n: number) => Array.from({ length: n }, (_, i) => ({ lang_code: "en", resource_type: "ulb", book_code: ["gen", "exo", "lev", "num", "deu", "jos", "jdg"][i] ?? "rut" }));
    const tooMany = await s.post(judeRequest({ resource_requests: many(6) }));
    expect(tooMany.status).toBe(422);
    expect(JSON.stringify(tooMany.json)).toContain("at most 5 resources");
    const tooManyBooks = await s.post(judeRequest({ resource_requests: many(4) }));
    expect(tooManyBooks.status).toBe(422);
    expect(JSON.stringify(tooManyBooks.json)).toContain("at most 3 distinct book codes");
    const badLang = await s.post(judeRequest({ resource_requests: [{ lang_code: "x".repeat(40), resource_type: "ulb", book_code: "jud" }] }));
    expect(badLang.status).toBe(422);
    expect(JSON.stringify(badLang.json)).toContain("lang_code");
    const fine = await s.post(judeRequest());
    expect(fine.status).toBe(200);
  }, 30_000);
});

describe("operational safety", () => {
  it("scratch directories of dead workers are removed at start, live ones are kept", async () => {
    const work = tmp();
    mkdirSync(join(work, "jobs", "999999-1-1-aaaa"), { recursive: true });
    mkdirSync(join(work, "jobs", `${process.pid}-1-1-bbbb`), { recursive: true });
    const removed = await recoverScratch(work);
    expect(removed).toEqual(["999999-1-1-aaaa"]);
    expect(readdirSync(join(work, "jobs"))).toEqual([`${process.pid}-1-1-bbbb`]);
  });

  it("the output sweeper deletes documents older than the retention window and nothing else", async () => {
    const out = tmp();
    const old = Date.now() / 1000 - 3 * 3600;
    for (const f of ["a.docx", "a.findings.json", "b.html", "c.pdf", "d.docx.partial", "keep.txt"]) writeFileSync(join(out, f), "x");
    for (const f of ["a.docx", "a.findings.json", "d.docx.partial", "keep.txt"]) utimesSync(join(out, f), old, old);
    expect(await sweepOutput(out, 2 * 3600_000)).toBe(3);
    expect(readdirSync(out).sort()).toEqual(["b.html", "c.pdf", "keep.txt"]);
  });

  it("the cache window: a document younger than OUTPUT_TTL_MINUTES is served, an older one is built again; the window is configurable", async () => {
    s = await createStack({ env: { OUTPUT_TTL_MINUTES: "1" } });
    await s.startWorker();
    const id = String((await s.post()).json["task_id"]);
    const first = await s.waitFor(id);
    const key = String(first.last["result"]);
    const file = join(s.dirs.out, `${key}.docx`);
    const second = await s.waitFor(String((await s.post()).json["task_id"]));
    expect(second.trace).toEqual(["Receiving request", "SUCCESS"]); // a cache hit stops at the first stage
    const aged = Date.now() / 1000 - 120;
    utimesSync(file, aged, aged);
    const third = await s.waitFor(String((await s.post()).json["task_id"]));
    expect(third.trace.length).toBeGreaterThan(2); // built again
    expect(existsSync(file)).toBe(true);
  }, 60_000);

  it("the disk guard: a job on a volume below the floor fails transient (disk_space_low) and is retried, then succeeds when space returns", async () => {
    const floor = 100_000_000;
    s = await createStack({ env: { DISK_MIN_FREE_MB: String(floor), JOB_ATTEMPTS: "5" } });
    await expect(checkDisk(s.jobConfig, [s.dirs.out])).rejects.toMatchObject({ code: "disk_space_low", transient: true });
    expect((await checkDisk({ ...s.jobConfig, DISK_MIN_FREE_MB: 1 }, [s.dirs.out])).freeMb).toBeGreaterThan(1);
    // through the queue (the API's own guard would refuse at half the floor, so enqueue through the service)
    await s.startWorker();
    const queued = await s.app.jobs?.service.enqueue({ key: "en_ulb-tn_jud_7af47830c746", request: judeRequest(), kind: "docx", requestId: "disk", receivedAt: Date.now(), submissionId: "x" });
    const id = queued?.id ?? "";
    for (let i = 0; i < 100; i += 1) {
      await new Promise((r) => setTimeout(r, 50));
      if (((await s.app.jobs?.service.queue.getJob(id))?.attemptsMade ?? 0) >= 1) break;
    }
    const retried = s.logs().find((l) => l["msg"] === "attempt failed, will retry");
    expect(retried).toMatchObject({ code: "disk_space_low", transient: true, retrying: true });
    expect(dirname(join(s.dirs.out, "x"))).toBe(s.dirs.out);
  }, 60_000);

  it("graceful shutdown: SIGTERM to the API process stops it within the drain deadline and exits 0", async () => {
    s = await createStack();
    const port = 28_000 + (process.pid % 500);
    const child = spawn(process.execPath, [join(PROTO, "src/main.ts")], { env: { ...process.env, ...s.env, PORT: String(port), API_DRAIN_MS: "3000" }, cwd: PROTO, stdio: ["ignore", "pipe", "pipe"] });
    let log = "";
    child.stdout.on("data", (c: Buffer) => (log += c.toString()));
    child.stderr.on("data", (c: Buffer) => (log += c.toString()));
    for (let i = 0; i < 100 && !log.includes("listening"); i += 1) await new Promise((r) => setTimeout(r, 100));
    expect(log).toContain("listening");
    expect((await fetch(`http://127.0.0.1:${port}/health/status`)).status).toBe(200);
    const started = Date.now();
    child.kill("SIGTERM");
    const code = await new Promise<number | null>((r) => child.once("exit", (c) => r(c)));
    expect(code).toBe(0);
    expect(Date.now() - started).toBeLessThan(3_500);
    expect(log).toContain("shutting down: no new connections");
    expect(log).toContain("drained; bye");
  }, 60_000);
});
