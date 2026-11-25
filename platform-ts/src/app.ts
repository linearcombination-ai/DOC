import { OpenAPIHono } from "@hono/zod-openapi";
import type { Context, MiddlewareHandler } from "hono";
import { z } from "zod";
import type { Config } from "./config.ts";
import * as r from "./contract/routes.ts";
import * as s from "./contract/schemas.ts";
import { Lookup, type Mode, type Outcome } from "./service/lookup.ts";
import { isDocError } from "./util/errors.ts";
import type { JobConfig } from "./jobs/config.ts";
import { JobError } from "./jobs/errors.ts";
import type { JobCore } from "./jobs/jobcore.ts";
import type { JobService } from "./jobs/queue.ts";
import { findingsPath } from "./jobs/pipeline.ts";
import { readFile } from "node:fs/promises";
import { randomUUID } from "node:crypto";
import { bodyLimit } from "hono/body-limit";
import { checkDisk } from "./jobs/pipeline.ts";
import { collectFleet, type Telemetry } from "./obs/telemetry.ts";
import { renderPrometheus, noMetrics, RedisMetrics, type Gauge, type Metrics } from "./obs/metrics.ts";
import type { Log } from "./util/log.ts";

interface Env {
  Variables: { mode: Mode; reqLog: Log; reqId: string };
}

/** A caller-supplied request id is kept when it is short and plain (it goes into log lines); otherwise a new one is made. */
const REQUEST_ID = /^[A-Za-z0-9._:-]{1,64}$/;

/** FastAPI's `RequestValidationError` body from DOC's `app.py` handler. */
function legacyValidationBody(message: string) {
  return { status_code: 10422 as const, message, data: null };
}

const ROUTE_PATHS = ["/health/status", "/chapters_in_books", "/language_codes_and_names", "/book_codes_for_lang/:lang_code", "/book_codes_for_lang_from_usfm_only/:lang_code", "/shared_book_codes/:lang0_code/:lang1_code", "/resource_types/:lang_code/:book_codes"];

export interface JobDeps {
  service: JobService;
  core: JobCore;
  config: JobConfig;
  metrics: Metrics;
  telemetry: Telemetry;
}

export function createApp(deps: { lookup: Lookup; config: Config; log: Log; jobs?: JobDeps; metrics?: Metrics }) {
  const { lookup, config, log, jobs } = deps;
  const metrics = deps.metrics ?? noMetrics;
  const maxBooks = jobs?.config.MAX_BOOK_CODES ?? 100;
  const strictBooks = s.strictBookCodes(maxBooks);
  const app = new OpenAPIHono<Env>({
    strict: true, // `/x/` is not `/x`: the redirect below reproduces Starlette's redirect_slashes
    defaultHook: (result, c) => {
      if (!result.success) {
        return c.json(legacyValidationBody(result.error.issues.map((i) => `${i.path.join(".")}: ${i.message}`).join("; ")), 422);
      }
      return undefined;
    },
  });

  // --- cross-cutting: request id and log, mode, CORS (same origins and settings as DOC's FastAPI)
  app.use("*", async (c, next) => {
    const started = performance.now();
    const requestedMode = c.req.header("x-doc-mode");
    const mode: Mode = config.ALLOW_MODE_HEADER && (requestedMode === "as_built" || requestedMode === "fixed") ? requestedMode : config.DOC_MODE;
    c.set("mode", mode);
    const supplied = c.req.header("x-request-id");
    const reqId = supplied !== undefined && REQUEST_ID.test(supplied) ? supplied : randomUUID().slice(0, 12);
    c.set("reqId", reqId);
    const reqLog = log.child({ component: "http", reqId, mode });
    c.set("reqLog", reqLog);
    reqLog.debug({ method: c.req.method, path: c.req.path, query: c.req.query() }, "request");
    await next();
    c.res.headers.set("x-request-id", reqId);
    const ms = Math.round(performance.now() - started);
    reqLog.info({ method: c.req.method, path: c.req.path, status: c.res.status, ms }, "response");
    if (c.req.path !== "/metrics") metrics.inc("doc_http_requests_total", { route: c.req.routePath === "/*" || c.req.routePath === "*" ? "other" : c.req.routePath, status: `${Math.floor(c.res.status / 100)}xx` });
  });
  // Starlette: HEAD is not allowed on a GET route (Hono would answer it from the GET handler), and it matches
  // routes on the *decoded* path, so `%2F` adds a segment instead of staying inside a parameter.
  app.use("*", async (c, next) => {
    if (c.req.method === "HEAD" && ROUTE_PATHS.some((p) => matches(p, c.req.path))) {
      return c.body(null, 405, { "content-type": "application/json", allow: "GET" });
    }
    if (/%2f/i.test(new URL(c.req.url).pathname)) return c.json({ detail: "Not Found" }, 404);
    await next();
    return undefined;
  });
  app.use("*", starletteCors(config.corsOrigins));
  // Starlette redirects `/x/` to `/x` (307) when only the slashless route exists.
  app.use("*", async (c, next) => {
    const path = c.req.path;
    if (path.length > 1 && path.endsWith("/") && ROUTE_PATHS.some((p) => matches(p, path.slice(0, -1)))) {
      const url = new URL(c.req.url);
      url.pathname = path.slice(0, -1);
      return c.redirect(url.toString(), 307);
    }
    await next();
    return undefined;
  });

  const startedAtMs = Date.now();
  const deadline = (): number => Date.now() + config.REQUEST_DEADLINE_MS;

  function answer<T>(c: Context<Env>, outcome: Outcome<T>, findingsEnvelope: boolean) {
    c.header("x-doc-mode", c.get("mode"));
    c.header("x-doc-findings", String(outcome.findings.length));
    if (outcome.stale) c.header("x-doc-stale", "1");
    if (findingsEnvelope) {
      return c.json({
        result: outcome.body,
        findings: outcome.findings,
        stale: outcome.stale,
        failedRepos: outcome.failedRepos.map((f) => ({ url: f.url, reason: f.reason, detail: f.detail, attempts: f.attempts })),
      }, 200);
    }
    return c.json(outcome.body, 200);
  }

  /** `fixed` mode checks path parameters beyond "non-empty" and answers in the legacy 422 shape. */
  function strictParam<Schema extends z.ZodType>(c: Context<Env>, schema: Schema, name: string, value: string) {
    if (c.get("mode") === "fixed" && !schema.safeParse(value).success) {
      return c.json(legacyValidationBody(`${name}: ${JSON.stringify(value)} is not an accepted ${name}`), 422);
    }
    return undefined;
  }

  app.openapi(r.health, async (c) => {
    if (!jobs) return c.json([{ status: "ok" as const }, 200 as const], 200);
    const body = await healthBody(jobs, startedAtMs);
    if (c.req.valid("query").strict === "1" && body.status !== "ok") return c.json([body, 200 as const], 503);
    return c.json([body, 200 as const], 200);
  });
  app.openapi(r.chaptersInBooks, (c) => c.json(lookup.chapters(), 200));

  app.openapi(r.languageCodesAndNames, async (c) => {
    const out = await lookup.languages(c.get("mode"));
    return answer(c, out, c.req.valid("query").findings === "1");
  });
  app.openapi(r.bookCodesForLang, async (c) => {
    const { lang_code } = c.req.valid("param");
    const bad = strictParam(c, s.StrictLangCode, "lang_code", lang_code);
    if (bad) return bad;
    return answer(c, await lookup.bookCodes(lang_code, false, c.get("mode"), deadline()), c.req.valid("query").findings === "1");
  });
  app.openapi(r.bookCodesForLangFromUsfmOnly, async (c) => {
    const { lang_code } = c.req.valid("param");
    const bad = strictParam(c, s.StrictLangCode, "lang_code", lang_code);
    if (bad) return bad;
    return answer(c, await lookup.bookCodes(lang_code, true, c.get("mode"), deadline()), c.req.valid("query").findings === "1");
  });
  app.openapi(r.sharedBookCodes, async (c) => {
    const { lang0_code, lang1_code } = c.req.valid("param");
    const bad = strictParam(c, s.StrictLangCode, "lang0_code", lang0_code) ?? strictParam(c, s.StrictLangCode, "lang1_code", lang1_code);
    if (bad) return bad;
    return answer(c, await lookup.sharedBookCodes(lang0_code, lang1_code, c.get("mode"), deadline()), c.req.valid("query").findings === "1");
  });
  app.openapi(r.resourceTypes, async (c) => {
    const { lang_code, book_codes } = c.req.valid("param");
    const bad = strictParam(c, s.StrictLangCode, "lang_code", lang_code) ?? strictParam(c, strictBooks, "book_codes", book_codes);
    if (bad) return bad;
    return answer(c, await lookup.resourceTypes(lang_code, book_codes, c.get("mode"), deadline()), c.req.valid("query").findings === "1");
  });

  if (jobs) {
    const j: JobDeps = jobs;
    const LANG = s.StrictLangCode;
    const BOOK = /^[a-z0-9]{3}$/;
    const TYPE = /^[a-z0-9_-]{1,20}$/;
    /** `fixed` mode: the captain's path validation defaults applied to the body: language 1 to 35 letters, digits and hyphens; three-character book codes; bounded counts. */
    function strictBody(body: Record<string, unknown>): string | undefined {
      const list = body["resource_requests"];
      if (!Array.isArray(list)) return undefined; // the core reports the schema problem
      if (list.length > j.config.MAX_RESOURCE_REQUESTS) return `resource_requests: at most ${j.config.MAX_RESOURCE_REQUESTS} resources in one request (got ${list.length})`;
      const books = new Set<string>();
      for (const [i, r] of list.entries()) {
        const o = (r ?? {}) as Record<string, unknown>;
        if (typeof o["lang_code"] === "string" && !LANG.safeParse(o["lang_code"]).success) return `resource_requests[${i}].lang_code: ${JSON.stringify(o["lang_code"])} is not an accepted lang_code (1 to 35 letters, digits and hyphens)`;
        if (typeof o["book_code"] === "string" && !BOOK.test(o["book_code"])) return `resource_requests[${i}].book_code: ${JSON.stringify(o["book_code"])} is not a three-character lowercase book code`;
        if (typeof o["resource_type"] === "string" && !TYPE.test(o["resource_type"])) return `resource_requests[${i}].resource_type: ${JSON.stringify(o["resource_type"])} is not an accepted resource type`;
        if (typeof o["book_code"] === "string") books.add(o["book_code"]);
      }
      if (books.size > j.config.MAX_BOOK_CODES) return `resource_requests: at most ${j.config.MAX_BOOK_CODES} distinct book codes in one request (got ${books.size})`;
      return undefined;
    }
    const limit = bodyLimit({
      maxSize: j.config.MAX_BODY_BYTES,
      onError: (c) => c.json({ code: "request_too_large" as const, message: `the request body is larger than ${j.config.MAX_BODY_BYTES} bytes`, detail: `the request body is larger than ${j.config.MAX_BODY_BYTES} bytes`, retryable: false as const }, 413),
    });
    app.use("/documents", limit);
    app.use("/documents_docx", limit);
    async function enqueue(c: Context<Env>, body: Record<string, unknown>, kind: "docx" | "document") {
      // the cheap bounds first (counts and character classes), then the core's validation with DOC's rules
      if (c.get("mode") === "fixed") {
        const problem = strictBody(body);
        if (problem !== undefined) return { response: c.json(legacyValidationBody(problem), 422) };
      }
      const keyed = j.core.key(body);
      if (!keyed.ok) return { response: c.json(legacyValidationBody(keyed.message), 422) };
      if (kind === "docx" && body["generate_docx"] !== true) return { response: c.json(legacyValidationBody("generate_docx: must be true for /documents_docx"), 422) };
      // refuse new work while the disk is nearly full: half the floor the workers use (a job already waiting keeps its place)
      try {
        await checkDisk({ ...j.config, DISK_MIN_FREE_MB: Math.floor(j.config.DISK_MIN_FREE_MB / 2) }, [j.config.DOCUMENT_OUTPUT_DIR]);
      } catch (error) {
        if (error instanceof JobError && error.code === "disk_space_low") {
          c.get("reqLog").error({ err: error.message }, "refusing a new job: disk space is low");
          return { response: c.json({ code: "disk_space_low" as const, message: error.message, detail: error.message, retryable: true as const }, 503, { "retry-after": "60" }) };
        }
        throw error;
      }
      const queued = await j.service.enqueue({ key: keyed.key, request: body, kind, requestId: c.get("reqId"), receivedAt: Date.now(), submissionId: randomUUID() });
      c.header("x-doc-deduplicated", queued.deduplicated ? "1" : "0");
      return { id: queued.id };
    }
    app.openapi(r.postDocumentsDocx, async (c) => {
      const out = await enqueue(c, c.req.valid("json"), "docx");
      return "response" in out ? out.response : c.json({ task_id: out.id }, 200);
    });
    app.openapi(r.postDocuments, async (c) => {
      const body = c.req.valid("json");
      if (body["generate_pdf"] === true && j.config.PDF_ENGINE === "off") return c.json({ code: "not_implemented" as const, message: "PDF output is switched off on this service (PDF_ENGINE=off); the html is available without generate_pdf", detail: "PDF output is switched off on this service (PDF_ENGINE=off)", retryable: false as const }, 501);
      const out = await enqueue(c, body, "document");
      return "response" in out ? out.response : c.json({ task_id: out.id }, 200);
    });
    app.openapi(r.metrics, async (c) => {
      const gauges: Gauge[] = [];
      let counters = new Map<string, number>();
      try {
        const redisMetrics = j.metrics instanceof RedisMetrics ? j.metrics : undefined;
        if (redisMetrics) counters = await redisMetrics.snapshot();
        const fleet = await collectFleet(j.service.queue, j.telemetry);
        gauges.push(...fleet.gauges);
        if (redisMetrics) gauges.push({ name: "doc_metrics_dropped_total", labels: {}, value: redisMetrics.dropped });
      } catch {
        gauges.push({ name: "doc_redis_up", labels: {}, value: 0 });
      }
      return c.text(renderPrometheus(counters, gauges), 200, { "content-type": "text/plain; version=0.0.4; charset=utf-8" });
    });
    app.openapi(r.taskStatus, async (c) => {
      const status = await j.service.status(c.req.valid("param").task_id);
      if (status.state === "SUCCESS" && typeof status.result === "string") {
        const n = await readFile(findingsPath(j.config.DOCUMENT_OUTPUT_DIR, status.result), "utf8").then((t) => (JSON.parse(t) as unknown[]).length).catch(() => 0);
        c.header("x-doc-findings", String(n));
      }
      return c.json(status as z.infer<typeof s.TaskStatus>, 200);
    });
    app.openapi(r.documentFindings, async (c) => {
      try {
        const text = await readFile(findingsPath(j.config.DOCUMENT_OUTPUT_DIR, c.req.valid("param").key), "utf8");
        c.header("x-doc-findings", String((JSON.parse(text) as unknown[]).length));
        return c.json(JSON.parse(text) as Record<string, unknown>[], 200);
      } catch {
        return c.json({ detail: "Not Found" as const }, 404);
      }
    });
  }

  // --- the rest of what FastAPI answers for these paths
  for (const path of ROUTE_PATHS) {
    app.on(["POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"], path, (c) => c.json({ detail: "Method Not Allowed" }, 405, { allow: "GET" }));
  }
  app.notFound((c) => c.json({ detail: "Not Found" }, 404));
  app.onError((error, c) => {
    const reqLog = c.get("reqLog");
    if (error instanceof JobError) {
      reqLog.error({ code: error.code, err: error.message }, "request failed with a typed job error");
      return c.json({ code: "queue_unavailable" as const, message: error.message, detail: error.message, retryable: true as const }, 503, { "retry-after": "5" });
    }
    if (isDocError(error)) {
      reqLog.error({ kind: error.kind, detail: error.detail, err: error.message }, "request failed with a typed error");
      const kind = error.kind === "deadline_exceeded" ? "deadline_exceeded" : "catalog_unavailable";
      return c.json({ kind, message: error.message, transient: true as const, detail: error.detail }, 503, { "retry-after": "5" });
    }
    reqLog.error({ err: error instanceof Error ? error.stack : String(error) }, "unhandled error");
    return c.text("Internal Server Error", 500);
  });

  app.doc31("/openapi.json", { openapi: "3.1.0", info: { title: "DOC shell (TypeScript), waves 1-3", version: "0.0.4", description: "Experimental TypeScript shell running beside DOC's FastAPI. Endpoints 1-7 of the stack report's table." } });
  return app;
}

/**
 * CORS exactly as Starlette's CORSMiddleware(allow_origins=[...], allow_credentials=True,
 * allow_methods=["GET","POST"], allow_headers=["*"]) answers, because Hono's own `cors` adds
 * `access-control-allow-credentials` to every response, with or without an Origin (found by the
 * parity comparison, not assumed).
 */
function starletteCors(allowed: string[]): MiddlewareHandler<Env> {
  const methods = "GET, POST";
  const safelisted = ["Accept", "Accept-Language", "Content-Language", "Content-Type"];
  return async (c, next) => {
    const origin = c.req.header("origin");
    if (origin === undefined) return next();
    const originAllowed = allowed.includes(origin);
    if (c.req.method === "OPTIONS" && c.req.header("access-control-request-method") !== undefined) {
      const headers: Record<string, string> = { "access-control-allow-methods": methods, "access-control-max-age": "600", "access-control-allow-credentials": "true", vary: "Origin" };
      if (originAllowed) headers["access-control-allow-origin"] = origin;
      const requested = c.req.header("access-control-request-headers");
      headers["access-control-allow-headers"] = requested ?? [...safelisted, "*"].sort().join(", ");
      const failures: string[] = [];
      if (!originAllowed) failures.push("origin");
      if (!methods.split(", ").includes(c.req.header("access-control-request-method") ?? "")) failures.push("method");
      if (failures.length > 0) return c.text(`Disallowed CORS ${failures.join(", ")}`, 400, headers);
      return c.text("OK", 200, headers);
    }
    await next();
    c.res.headers.set("access-control-allow-credentials", "true");
    if (originAllowed) {
      c.res.headers.set("access-control-allow-origin", origin);
      c.res.headers.append("vary", "Origin");
    }
    return undefined;
  };
}

/** Match a Hono-style pattern (`:param`) against a concrete path. */
function matches(pattern: string, path: string): boolean {
  const a = pattern.split("/");
  const b = path.split("/");
  return a.length === b.length && a.every((seg, i) => seg.startsWith(":") || seg === b[i]);
}

/** Everything `/health/status` reports for a configured queue, and why it is degraded when it is. Never throws: a dead Redis is a finding, not an error. */
async function healthBody(j: JobDeps, _startedAt: number): Promise<z.infer<typeof s.HealthBody>> {
  const problems: string[] = [];
  const queue = await j.service.health();
  const body: z.infer<typeof s.HealthBody> = { status: "ok", queue };
  if (queue.redis === "down") {
    problems.push(`Redis is unreachable: ${queue.error ?? "no answer"}`);
    body.redis = { up: false };
  } else {
    const started = performance.now();
    const info = await j.service.connection.info().catch(() => "");
    const field = (name: string): string | undefined => new RegExp(`^${name}:(.*)$`, "m").exec(info)?.[1]?.trim();
    const policy = await j.service.connection.config("GET", "maxmemory-policy").then((r) => (Array.isArray(r) ? String(r[1]) : undefined), () => undefined);
    const saved = field("rdb_last_save_time");
    body.redis = {
      up: true,
      latencyMs: Number((performance.now() - started).toFixed(1)),
      ...(field("redis_version") ?? field("valkey_version") ? { version: String(field("valkey_version") ?? field("redis_version")) } : {}),
      ...(field("used_memory_human") ? { usedMemory: String(field("used_memory_human")) } : {}),
      ...(field("aof_enabled") !== undefined ? { aof: field("aof_enabled") === "1" } : {}),
      lastSaveAgeSeconds: saved ? Math.max(0, Math.floor(Date.now() / 1000) - Number(saved)) : null,
      ...(policy ? { evictionPolicy: policy } : {}),
      ...(field("connected_clients") ? { connectedClients: Number(field("connected_clients")) } : {}),
    };
    if (policy !== undefined && policy !== "noeviction") problems.push(`Redis maxmemory-policy is ${policy}; BullMQ needs noeviction (queue keys could be evicted)`);
    try {
      const fleet = await collectFleet(j.service.queue, j.telemetry);
      body.workers = fleet.workers.map((w) => ({ id: w.id, pid: w.pid, heartbeatAgeSeconds: Number(((Date.now() - w.at) / 1000).toFixed(1)), running: w.running, rssMb: Math.round(w.rssBytes / 1048576), docxMode: w.docxMode }));
      const first = fleet.workers[0];
      if (first) {
        const sum = fleet.workers.reduce((a, w) => ({ size: a.size + w.pool.size, idle: a.idle + w.pool.idle, busy: a.busy + w.pool.busy, starting: a.starting + w.pool.starting, recycled: a.recycled + w.pool.recycled, crashed: a.crashed + w.pool.crashed }), { size: 0, idle: 0, busy: 0, starting: 0, recycled: 0, crashed: 0 });
        body.docxPool = { mode: first.docxMode, ...sum };
      }
      if (fleet.workers.length === 0) problems.push("no worker has a fresh heartbeat: queued jobs will wait");
      if (fleet.oldestWaitingMs !== null && fleet.oldestWaitingMs > j.config.QUEUE_LAG_WARN_MS) problems.push(`the oldest waiting job is ${Math.round(fleet.oldestWaitingMs / 1000)} s old (warn at ${Math.round(j.config.QUEUE_LAG_WARN_MS / 1000)} s)`);
      const ages = Object.fromEntries(Object.entries(fleet.catalogLast).map(([k, at]) => [k, Math.round((Date.now() - at) / 1000)]));
      body.catalog = { lastSuccessAgeSeconds: ages };
      const warnAfter = j.config.CATALOG_STALE_WARN_SECONDS;
      if (warnAfter > 0 && fleet.workers.length > 0 && Object.values(ages).every((a) => a > warnAfter)) problems.push(`no catalog fetch has succeeded for over ${warnAfter} s`);
    } catch (error) {
      problems.push(`fleet state could not be read: ${error instanceof Error ? error.message : String(error)}`);
    }
  }
  const disk = await checkDisk({ ...j.config, DISK_MIN_FREE_MB: 0 }, [j.config.DOCUMENT_OUTPUT_DIR, j.config.WORKING_DIR]).catch(() => undefined);
  if (disk && Number.isFinite(disk.freeMb)) {
    body.disk = { freeMb: disk.freeMb, dir: disk.dir, floorMb: j.config.DISK_MIN_FREE_MB };
    if (j.config.DISK_MIN_FREE_MB > 0 && disk.freeMb < j.config.DISK_MIN_FREE_MB) problems.push(`only ${disk.freeMb} MB free on ${disk.dir} (floor ${j.config.DISK_MIN_FREE_MB} MB)`);
  }
  if (problems.length > 0) {
    body.status = "degraded";
    body.problems = problems;
  }
  return body;
}
