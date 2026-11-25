/**
 * P6 item C: one structured log line per stage and attempt with a request id from the HTTP request to the worker and the
 * Python process, the Prometheus endpoint, and the extended health status.
 */
import { afterEach, describe, expect, it } from "vitest";
import { Health } from "../src/contract/schemas.ts";
import { createStack, judeRequest, type Stack } from "./support/stack.ts";

/** The health body, parsed with the contract's own schema (so the tests also prove the response matches it). */
const healthOf = (text: string) => Health.parse(JSON.parse(text))[0];

let s: Stack | undefined;
afterEach(async () => {
  await s?.close();
  s = undefined;
});
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
async function until(what: string, test: () => Promise<boolean> | boolean, timeoutMs = 20_000): Promise<void> {
  const end = Date.now() + timeoutMs;
  while (Date.now() < end) {
    if (await test()) return;
    await sleep(50);
  }
  throw new Error(`timed out waiting for ${what}`);
}
const get = async (st: Stack, path: string): Promise<{ status: number; text: string; headers: Headers }> => {
  const res = await st.app.app.request(path);
  return { status: res.status, text: await res.text(), headers: res.headers };
};
const metricsText = async (st: Stack): Promise<string> => (await get(st, "/metrics")).text;
const SAMPLE = /^[a-zA-Z_:][a-zA-Z0-9_:]*(\{[^}]*\})? -?[0-9.e+-]+$/;

describe("request id", () => {
  for (const mode of ["process", "persistent"]) {
    it(`an x-request-id travels from the HTTP request to every job log line and to the Python process (${mode} mode)`, async () => {
      s = await createStack({ env: { DOCX_MODE: mode } });
      await s.startWorker();
      const res = await s.app.app.request("/documents_docx", { method: "POST", headers: { "content-type": "application/json", "x-request-id": "req-abc.123" }, body: JSON.stringify(judeRequest()) });
      expect(res.headers.get("x-request-id")).toBe("req-abc.123");
      const id = String(((await res.json()) as { task_id: string }).task_id);
      const done = await s.waitFor(id);
      expect(done.last["state"]).toBe("SUCCESS");
      const lines = s.logs().filter((l) => l["reqId"] === "req-abc.123" || l["req_id"] === "req-abc.123");
      const http = lines.filter((l) => l["component"] === "http");
      expect(http.length).toBeGreaterThanOrEqual(1);
      const stages = lines.filter((l) => l["msg"] === "stage finished").map((l) => l["stage"]);
      expect(stages).toEqual(["Locating assets", "Provisioning asset files", "Parsing asset files", "Assembling content", "Converting to Docx"]);
      for (const l of lines.filter((x) => x["msg"] === "stage finished")) expect(typeof l["ms"]).toBe("number");
      expect(lines.some((l) => l["msg"] === "attempt started" && l["attempt"] === 1)).toBe(true);
      expect(lines.some((l) => l["msg"] === "job finished")).toBe(true);
      const python = s.logs().filter((l) => String(l["msg"]).startsWith("python: ") && l["req_id"] === "req-abc.123");
      expect(python.map((l) => l["msg"])).toEqual(expect.arrayContaining(["python: render start", "python: render done"]));
      // nothing the job logged lacks the id
      const jobLines = s.logs().filter((l) => l["component"] === "job" && l["job"] === id);
      expect(jobLines.length).toBeGreaterThan(5);
      expect(jobLines.every((l) => l["reqId"] === "req-abc.123")).toBe(true);
    }, 60_000);
  }

  it("without a header a request id is generated and returned; an unsafe one is replaced", async () => {
    s = await createStack();
    const a = await s.app.app.request("/health/status");
    expect(a.headers.get("x-request-id")).toMatch(/^[0-9a-f-]{12}$/);
    const b = await s.app.app.request("/health/status", { headers: { "x-request-id": "bad id with spaces\n" } });
    expect(b.headers.get("x-request-id")).toMatch(/^[0-9a-f-]{12}$/);
  });

  it("each attempt is one line: a failing attempt names its attempt number, code and whether it will retry, with the same request id", async () => {
    s = await createStack({ env: { DOCX_FAULT: "crash", JOB_ATTEMPTS: "3" } });
    await s.startWorker();
    const res = await s.app.app.request("/documents_docx", { method: "POST", headers: { "content-type": "application/json", "x-request-id": "req-fail-1" }, body: JSON.stringify(judeRequest()) });
    const id = String(((await res.json()) as { task_id: string }).task_id);
    const done = await s.waitFor(id);
    expect(done.last["state"]).toBe("FAILURE");
    const attempts = s.logs().filter((l) => l["job"] === id && (l["msg"] === "attempt failed, will retry" || l["msg"] === "attempt failed, giving up"));
    expect(attempts.map((l) => [l["attempt"], l["retrying"], l["code"], l["reqId"]])).toEqual([
      [1, true, "docx_worker_crashed", "req-fail-1"],
      [2, true, "docx_worker_crashed", "req-fail-1"],
      [3, false, "docx_worker_crashed", "req-fail-1"],
    ]);
  }, 60_000);
});

describe("/metrics (Prometheus text format)", () => {
  it("after a good job and a permanent failure it reports queue, stages, attempts, failures by code, clones, catalog and workers", async () => {
    s = await createStack({ env: { HEARTBEAT_INTERVAL_MS: "200" } });
    await s.startWorker();
    const ok = String((await s.post()).json["task_id"]);
    expect((await s.waitFor(ok)).last["state"]).toBe("SUCCESS");
    const bad = String((await s.post(judeRequest({ resource_requests: [{ lang_code: "en", resource_type: "ulb", book_code: "3jn" }] }))).json["task_id"]);
    expect((await s.waitFor(bad)).last["state"]).toBe("FAILURE");
    await get(s, "/language_codes_and_names");
    await until("metrics to land", async () => {
      const t = await metricsText(s as Stack);
      return t.includes('doc_job_failures_total{code="resource_unavailable",retried="false"} 1') && t.includes("doc_workers_alive 1");
    });
    const text = await metricsText(s);
    for (const line of text.trim().split("\n")) expect(line.startsWith("#") || SAMPLE.test(line), `not Prometheus text: ${line}`).toBe(true);
    const must = [
      "# TYPE doc_stage_duration_seconds histogram",
      '# TYPE doc_job_attempts_total counter',
      'doc_job_attempts_total{result="ok"} 1',
      'doc_job_attempts_total{result="failed"} 1',
      'doc_job_failures_total{code="resource_unavailable",retried="false"} 1',
      'doc_stage_duration_seconds_count{ok="true",stage="Locating assets"} 2',
      'doc_stage_duration_seconds_bucket{le="+Inf",ok="true",stage="Converting to Docx"} 1',
      'doc_queue_jobs{state="waiting"} 0',
      'doc_queue_jobs{state="active"} 0',
      "doc_queue_oldest_waiting_seconds 0",
      "doc_redis_up 1",
      "doc_workers_alive 1",
      'doc_repo_clones_total{outcome="cloned"} 2',
      'doc_catalog_refreshes_total{kind="primary",result="ok"}',
      'doc_docx_jobs_total{mode="process",result="ok"} 1',
    ];
    for (const m of must) expect(text, m).toContain(m);
    expect(text).toMatch(/doc_worker_rss_bytes\{worker="[^"]+"\} \d+/);
    expect(text).toMatch(/doc_catalog_last_success_timestamp_seconds\{kind="primary"\} \d+/);
    expect(text).toMatch(/doc_http_requests_total\{route="\/language_codes_and_names",status="2xx"\} 1/);
    const lineage = text.split("\n").filter((l) => l.startsWith("doc_stage_duration_seconds_bucket") && l.includes('stage="Locating assets"'));
    const les = lineage.map((l) => /le="([^"]+)"/.exec(l)?.[1]);
    expect(les.at(-1)).toBe("+Inf"); // buckets are in increasing order
  }, 60_000);

  it("a second job for the same document is a cache hit and is counted as one; a worker that stops disappears from the liveness gauge", async () => {
    s = await createStack({ env: { HEARTBEAT_INTERVAL_MS: "200" } });
    const w = await s.startWorker();
    await s.waitFor(String((await s.post()).json["task_id"]));
    await s.waitFor(String((await s.post()).json["task_id"]));
    await until("cache hit counted", async () => (await metricsText(s as Stack)).includes('doc_job_attempts_total{result="cache_hit"} 1'));
    await w.close();
    await until("liveness gauge to drop", async () => (await metricsText(s as Stack)).includes("doc_workers_alive 0"));
  }, 60_000);

  it("Redis down: the endpoint still answers and says doc_redis_up 0", async () => {
    s = await createStack();
    await s.redis.stop();
    try {
      const res = await get(s, "/metrics");
      process.stderr.write(`DEBUG metrics with Redis down: ${res.status} ${res.text.slice(0, 300)}\n`);
      expect(res.text).toContain("doc_redis_up 0");
    } finally {
      await s.redis.restart();
    }
  }, 30_000);
});

describe("/health/status", () => {
  it("reports Redis, the queue, the workers, the docx pool, the catalog and the disk when everything is fine", async () => {
    s = await createStack({ env: { HEARTBEAT_INTERVAL_MS: "200", DOCX_MODE: "persistent" } });
    await s.startWorker();
    await s.waitFor(String((await s.post()).json["task_id"]));
    await until("an idle worker heartbeat", async () => {
      const w = healthOf((await get(s as Stack, "/health/status")).text).workers;
      return w?.length === 1 && w[0]?.running === 0;
    });
    const body = healthOf((await get(s, "/health/status")).text);
    expect(body["status"]).toBe("ok");
    expect(body["redis"]).toMatchObject({ up: true, aof: true });
    expect(body.redis?.version).toMatch(/^\d+\.\d+/);
    expect(typeof body.redis?.latencyMs).toBe("number");
    expect(body["queue"]).toMatchObject({ redis: "up", waiting: 0, active: 0 });
    expect(body.workers?.[0]).toMatchObject({ running: 0, docxMode: "persistent" });
    expect(body["docxPool"]).toMatchObject({ mode: "persistent", size: 2 });
    expect(body.catalog?.lastSuccessAgeSeconds["primary"]).toBeLessThan(60);
    expect(body.disk?.freeMb).toBeGreaterThan(0);
    expect(body["problems"]).toBeUndefined();
  }, 60_000);

  it("degraded with a reason when no worker is alive and a job has waited longer than the lag threshold; ?strict=1 answers 503", async () => {
    s = await createStack({ env: { QUEUE_LAG_WARN_MS: "200" } });
    await s.post();
    await sleep(400);
    const body = healthOf((await get(s, "/health/status")).text);
    expect(body["status"]).toBe("degraded");
    expect((body.problems ?? []).join(" | ")).toContain("no worker has a fresh heartbeat");
    expect((body.problems ?? []).join(" | ")).toContain("oldest waiting job");
    expect(body.queue?.waiting).toBe(1);
    expect((await get(s, "/health/status?strict=1")).status).toBe(503);
    expect((await get(s, "/health/status")).status).toBe(200);
  }, 30_000);

  it("Redis down: degraded, Redis reported down, and still HTTP 200 (liveness) unless strict", async () => {
    s = await createStack();
    await s.redis.stop();
    try {
      const res = await get(s, "/health/status");
      process.stderr.write(`DEBUG health with Redis down: ${res.status} ${res.text.slice(0, 400)}\n`);
      expect(res.status).toBe(200);
      const body = healthOf(res.text);
      expect(body["status"]).toBe("degraded");
      expect(body["redis"]).toEqual({ up: false });
      expect(body.problems?.[0]).toContain("Redis is unreachable");
    } finally {
      await s.redis.restart();
    }
  }, 30_000);

  it("the disk floor: degraded and a typed 503 on new jobs when the output volume is below it", async () => {
    s = await createStack({ env: { DISK_MIN_FREE_MB: "100000000" } });
    const body = healthOf((await get(s, "/health/status")).text);
    expect(body["status"]).toBe("degraded");
    expect((body.problems ?? []).join(" ")).toContain("free on");
    const res = await s.post();
    expect(res.status).toBe(503);
    expect(res.json).toMatchObject({ code: "disk_space_low", retryable: true });
    expect(res.headers.get("retry-after")).toBe("60");
  }, 30_000);
});
