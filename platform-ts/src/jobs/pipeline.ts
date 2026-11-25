/**
 * The document job: the stages DOC's `generate_docx_document` task runs, as one function with a deadline per stage.
 *
 *   Receiving request -> Locating assets -> Provisioning asset files -> Parsing asset files -> Assembling content
 *   -> Converting to Docx -> (placed atomically) -> SUCCESS with the document request key
 *
 * What each stage does here: locate = catalog lookup and the core's repository locator; provision = acquirer (git clone
 * cache); parse = core `job_parse` (TN reader, USFM model); assemble = core `job_assemble` (interleaver, proposed look);
 * docx = the Python worker over the DocxJob seam. Every stage is bounded; every failure is typed transient or permanent
 * (`errors.ts`); nothing is swallowed (findings travel with the result). The function has no Redis in it: the queue
 * adapter (`worker.ts`) supplies `JobContext`.
 */
import { readdir, readFile, statfs, writeFile } from "node:fs/promises";
import { join } from "node:path";
import type { Redis } from "ioredis";
import type { Acquirer } from "../acquirer/acquirer.ts";
import { pythonText } from "../acquirer/listing.ts";
import type { Config } from "../config.ts";
import type { LookupDto, Repo } from "../core/seam.ts";
import { noMetrics, type Metrics } from "../obs/metrics.ts";
import type { Lookup } from "../service/lookup.ts";
import type { Log } from "../util/log.ts";
import type { JobConfig } from "./config.ts";
import { DocxJob, validateDocx, type DocumentFormat, type DocxRunner } from "./docx.ts";
import { sendEmailOnce, type EmailDeps } from "./email.ts";
import { JobError } from "./errors.ts";
import type { Finding, JobCore } from "./jobcore.ts";
import { createScratch, isFresh, placeAtomically, removeScratch } from "./scratch.ts";
import { STAGE, type JobData, type Stage } from "./types.ts";

export interface JobContext {
  jobId: string;
  attempt: number;
  maxAttempts: number;
  /** Aborts when the attempt's overall ceiling passes. */
  signal: AbortSignal;
  setState(stage: Stage): Promise<void>;
}

export interface PipelineDeps {
  config: Config;
  jobs: JobConfig;
  log: Log;
  lookup: Lookup;
  acquirer: Acquirer;
  core: JobCore;
  /** The docx seam: one process per job, or the persistent pool (`DOCX_MODE`). */
  docx: DocxRunner;
  metrics?: Metrics;
  /** Redis for the email idempotency record; without it the email step is skipped with a finding. */
  redis?: Redis;
  /** Test hook: a transport for the email step instead of SMTP. */
  emailTransport?: EmailDeps["transport"];
  now?: () => number;
}

export interface JobOutcome {
  key: string;
  cacheHit: boolean;
  findings: number;
  timings: Record<string, number>;
}

interface RequestedResource {
  lang_code: string;
  resource_type: string;
  book_code: string;
}

const USFM_TYPES = new Set(["ulb", "udb", "reg", "blv", "cuv", "f10", "nav", "tbi", "ayt", "bpb", "ugnt", "uhb"]);

const EXTENSION: Record<DocumentFormat, string> = { docx: "docx", html: "html", pdf: "pdf" };
export function outputPath(dir: string, key: string, format: DocumentFormat): string {
  return join(dir, `${key}.${EXTENSION[format]}`);
}
export function documentPath(dir: string, key: string): string {
  return outputPath(dir, key, "docx");
}
export function findingsPath(dir: string, key: string): string {
  return join(dir, `${key}.findings.json`);
}

/** Which files a job produces, in the order they are placed. `docx`: the docx. `document`: the html DOC writes as a by-product and, when asked, the PDF. */
export function formatsFor(data: Pick<JobData, "kind" | "request">): DocumentFormat[] {
  if (data.kind === "docx") return ["docx"];
  return data.request["generate_pdf"] === true ? ["html", "pdf"] : ["html"];
}

const MIME: Record<DocumentFormat, string> = { docx: "application/vnd.openxmlformats-officedocument.wordprocessingml.document", html: "text/html", pdf: "application/pdf" };

/** Free space below the configured floor on the volumes a job writes to: a transient error, so the job waits (retries with backoff) instead of filling the disk. */
export async function checkDisk(jobs: JobConfig, dirs: readonly string[] = [jobs.WORKING_DIR, jobs.DOCUMENT_OUTPUT_DIR]): Promise<{ freeMb: number; dir: string }> {
  let lowest = { freeMb: Number.POSITIVE_INFINITY, dir: "" };
  for (const dir of dirs) {
    const fs = await statfs(dir).catch(() => undefined);
    if (!fs) continue;
    const freeMb = Math.floor((fs.bavail * fs.bsize) / (1024 * 1024));
    if (freeMb < lowest.freeMb) lowest = { freeMb, dir };
  }
  if (jobs.DISK_MIN_FREE_MB > 0 && lowest.freeMb < jobs.DISK_MIN_FREE_MB) {
    throw new JobError("disk_space_low", `only ${lowest.freeMb} MB are free on the volume of ${lowest.dir} (the floor is DISK_MIN_FREE_MB=${jobs.DISK_MIN_FREE_MB})`, { transient: true, detail: lowest });
  }
  return lowest;
}

/**
 * Run `work` under a stage deadline: whichever of the stage timer and the attempt's signal fires first aborts `signal` (which
 * `work` must honour: it kills its subprocess or terminates its thread). The stage then *waits for `work` to settle* (bounded by
 * `SETTLE_GRACE_MS`) before it rejects, so the caller's `finally` (deleting the scratch directory) never races a writer that is
 * still dying. A more specific error from `work` (for example `docx_worker_timeout`) wins over the generic stage error.
 */
const SETTLE_GRACE_MS = 5_000;
async function stage<T>(name: string, ms: number, parent: AbortSignal, work: (signal: AbortSignal) => Promise<T>): Promise<T> {
  const timer = AbortSignal.timeout(ms);
  const controller = new AbortController();
  const stop = (): void => controller.abort();
  timer.addEventListener("abort", stop, { once: true });
  parent.addEventListener("abort", stop, { once: true });
  if (timer.aborted || parent.aborted) stop();
  const running = work(controller.signal);
  let onAbort: (() => void) | undefined;
  const aborted = new Promise<"aborted">((resolve) => {
    onAbort = () => resolve("aborted");
    if (controller.signal.aborted) onAbort();
    else controller.signal.addEventListener("abort", onAbort, { once: true });
  });
  try {
    const first = await Promise.race([running.then((v) => ({ value: v })), aborted]);
    if (first !== "aborted") return first.value;
    const settled = await Promise.race([running.then((v) => ({ value: v }), (e: unknown) => ({ error: e })), new Promise<"grace">((r) => setTimeout(() => r("grace"), SETTLE_GRACE_MS).unref())]);
    if (settled !== "grace" && "value" in settled) return settled.value; // finished at the buzzer: keep the result
    if (settled !== "grace" && settled.error instanceof JobError) throw settled.error;
    throw new JobError(timer.aborted ? "stage_deadline" : "job_deadline", `${name} did not finish within ${timer.aborted ? ms : "the job"} ms`, { transient: true, detail: { stage: name, ms } });
  } finally {
    timer.removeEventListener("abort", stop);
    parent.removeEventListener("abort", stop);
    if (onAbort) controller.signal.removeEventListener("abort", onAbort);
  }
}

export async function runDocumentJob(deps: PipelineDeps, data: JobData, ctx: JobContext): Promise<JobOutcome> {
  const { jobs } = deps;
  const metrics = deps.metrics ?? noMetrics;
  const jlog = deps.log.child({ component: "job", job: ctx.jobId, key: data.key, reqId: data.requestId, attempt: ctx.attempt, of: ctx.maxAttempts });
  const timings: Record<string, number> = {};
  /** One log line and one histogram sample per stage, with the request id: the unit an operator greps for. */
  const timed = async <T>(name: Stage, work: () => Promise<T>): Promise<T> => {
    const started = performance.now();
    let ok = false;
    try {
      const value = await work();
      ok = true;
      return value;
    } finally {
      const ms = Math.round(performance.now() - started);
      timings[name] = ms;
      metrics.observe("doc_stage_duration_seconds", { stage: name, ok: String(ok) }, ms / 1000);
      jlog.info({ stage: name, ms, ok }, "stage finished");
    }
  };

  await ctx.setState(STAGE.receiving);
  const keyed = deps.core.key(data.request);
  if (!keyed.ok) throw new JobError("invalid_request", `the request is not valid: ${keyed.message}`, { transient: false });
  if (keyed.key !== data.key) throw new JobError("request_key_mismatch", `the worker computed ${keyed.key} but the job carries ${data.key}`, { transient: false });
  const requests = readRequests(data.request);
  const formats = formatsFor(data);
  const primary = formats.at(-1) ?? "docx";
  const finalPath = outputPath(jobs.DOCUMENT_OUTPUT_DIR, data.key, primary);
  const email = typeof data.request["email_address"] === "string" && data.request["email_address"] !== "" ? String(data.request["email_address"]) : undefined;
  const extraFindings: Finding[] = [];
  if (data.request["generate_epub"] === true) extraFindings.push({ code: "EpubNotSupported", severity: "warn", detail: "ePub output was removed from DOC (it needed Calibre); the request's generate_epub is ignored" });
  if (email !== undefined && !jobs.SEND_EMAIL) extraFindings.push({ code: "EmailDisabled", severity: "info", detail: "the request has an email address but this service has SEND_EMAIL off; no email is sent" });

  if (await isFresh(finalPath, jobs.OUTPUT_TTL_MINUTES * 60_000, deps.now?.())) {
    jlog.info({ path: finalPath }, "cache hit: a fresh document already exists");
    if (email !== undefined && jobs.EMAIL_ON_CACHE_HIT) await emailStep(deps, data, ctx, email, formats, [], jlog, timed);
    return { key: data.key, cacheHit: true, findings: 0, timings };
  }

  await checkDisk(jobs);
  const scratch = await createScratch(jobs.WORKING_DIR, ctx.jobId, ctx.attempt).catch((e: unknown) => {
    throw new JobError("scratch_io", `cannot create a scratch directory: ${e instanceof Error ? e.message : String(e)}`, { transient: true, cause: e });
  });
  try {
    // --- Locating assets
    await ctx.setState(STAGE.locating);
    const found = await timed(STAGE.locating, () => stage(STAGE.locating, jobs.STAGE_LOCATE_DEADLINE_MS, ctx.signal, () => locate(deps, requests)));
    if (found.resolved.length === 0) throw new JobError("no_resources", "none of the requested resources exists in the catalog", { transient: false, detail: { requests } });

    // --- Provisioning asset files
    await ctx.setState(STAGE.provisioning);
    const provisioned = await timed(STAGE.provisioning, () => stage(STAGE.provisioning, jobs.STAGE_PROVISION_DEADLINE_MS, ctx.signal, (signal) => provision(deps, found.resolved, signal, ctx.attempt >= ctx.maxAttempts)));

    // --- Parsing asset files
    await ctx.setState(STAGE.parsing);
    const parsed = await timed(STAGE.parsing, () =>
      stage(STAGE.parsing, jobs.STAGE_PARSE_DEADLINE_MS, ctx.signal, async (signal) => {
        const { findings: inputFindings, ...input } = await buildParseInput(data.request, found.resolved.filter((r) => !provisioned.missing.has(r.dto.url)), provisioned);
        const out = await deps.core.parse({ ...input, bug_compat: "fixed" }, signal);
        return { ...out, findings: [...inputFindings, ...out.findings] };
      }),
    );

    // --- Assembling content (HTML and PDF in DOC's own markup unless OUTPUT_LOOK=proposed; the docx always uses the proposed look's styles)
    const look = jobs.OUTPUT_LOOK;
    await ctx.setState(STAGE.assembling);
    const assembled = await timed(STAGE.assembling, () =>
      stage(STAGE.assembling, jobs.STAGE_ASSEMBLE_DEADLINE_MS, ctx.signal, (signal) =>
        deps.core.assemble({ request: data.request, books: parsed.books, structure: look === "doc" && data.kind !== "docx" ? "as_built" : "proposed_look", bug_compat: "fixed", unresolved: found.unresolved, findings: parsed.findings }, signal),
      ),
    );
    const findings: Finding[] = [...found.findings, ...provisioned.findings, ...assembled.findings, ...extraFindings];

    // --- Converting: one seam call per output format (docx; or html and then pdf)
    const book = requests.find((r) => USFM_TYPES.has(r.resource_type))?.book_code ?? requests[0]?.book_code ?? "";
    const produced: { format: DocumentFormat; tmp: string }[] = [];
    for (const format of formats) {
      if (format === "docx") await ctx.setState(STAGE.docx);
      else if (format === "pdf") await ctx.setState(STAGE.pdf);
      const tmp = join(scratch, `out.${EXTENSION[format]}`);
      await timed(format === "pdf" ? STAGE.pdf : STAGE.docx, () =>
        stage(format === "pdf" ? STAGE.pdf : STAGE.docx, jobs.STAGE_DOCX_DEADLINE_MS, ctx.signal, async (signal) => {
          const job: DocxJob = {
            version: 2,
            format,
            request_id: data.requestId,
            job_id: ctx.jobId,
            request_key: data.key,
            parts: assembled.parts.map((p) => ({ ...p, content: typeof p["content"] === "string" ? p["content"] : "" })),
            layout_for_print: data.request["layout_for_print"] === true,
            title1: parsed.title1,
            title2: parsed.title2,
            book,
            output_path: tmp,
            look: format === "docx" ? "proposed" : look,
            layout: typeof data.request["assembly_layout_kind"] === "string" ? data.request["assembly_layout_kind"] : "1c",
            pdf_engine: jobs.PDF_ENGINE === "prince" ? "prince" : jobs.PDF_ENGINE === "weasyprint" ? "weasyprint" : "auto",
            use_prince: data.request["use_prince"] === true,
          };
          const jobFile = join(scratch, `docx-job-${format}.json`);
          await writeFile(jobFile, JSON.stringify(DocxJob.parse(job)));
          const report = await deps.docx.run({ jobFile, deadlineMs: jobs.STAGE_DOCX_DEADLINE_MS, signal, requestId: data.requestId, memoryCeilingMb: format === "pdf" ? jobs.PDF_MEMORY_CEILING_MB : jobs.DOCX_MEMORY_CEILING_MB });
          jlog.info({ format, mode: deps.docx.mode, pid: report.pid, wallMs: report.wallMs, workerMs: report.result.ms, rssKb: report.result.rss_kb, bytes: report.result.bytes, engine: report.result.engine }, "document worker finished");
          if (report.result.note) findings.push({ code: "PdfEngineFallback", severity: "warn", detail: report.result.note });
          await validateDocx(tmp, format);
        }),
      );
      produced.push({ format, tmp });
    }

    // --- Placement: findings first, then the documents (a reader that sees a document can rely on its findings)
    await writeFile(join(scratch, "findings.json"), JSON.stringify(findings));
    await placeAtomically(join(scratch, "findings.json"), findingsPath(jobs.DOCUMENT_OUTPUT_DIR, data.key));
    for (const p of produced) await placeAtomically(p.tmp, outputPath(jobs.DOCUMENT_OUTPUT_DIR, data.key, p.format));
    jlog.info({ formats, findings: findings.length, timings }, "document placed");

    // --- Email: after the document exists, never allowed to fail it
    if (email !== undefined) {
      const sent = await emailStep(deps, data, ctx, email, formats, findings, jlog, timed);
      if (sent.length > 0) await writeFile(findingsPath(jobs.DOCUMENT_OUTPUT_DIR, data.key), JSON.stringify([...findings, ...sent]));
      await afterEmailFault(jobs.JOB_FAULT);
    }
    return { key: data.key, cacheHit: false, findings: findings.length, timings };
  } finally {
    await removeScratch(scratch).catch((e: unknown) => jlog.warn({ err: e instanceof Error ? e.message : String(e), scratch }, "could not remove the scratch directory"));
  }
}

/** Test-only fault: the attempt dies right after the email step (once), as a crash or a lost lock would, so the retry meets an email already sent. */
async function afterEmailFault(spec: string): Promise<void> {
  if (!spec.startsWith("after_email_once:")) return;
  const marker = spec.slice("after_email_once:".length);
  const { existsSync, writeFileSync } = await import("node:fs");
  if (existsSync(marker)) return;
  writeFileSync(marker, "fired");
  throw new JobError("internal", "injected fault after the email step", { transient: true });
}

/**
 * The email step. It runs after the document is placed and cannot fail the job: whatever happens becomes a finding (and a metric).
 * Idempotency lives in `email.ts` (a record per submission, recipient and document), so a retry or a redelivery never sends again.
 * Returns the findings it added.
 */
async function emailStep(
  deps: PipelineDeps,
  data: JobData,
  ctx: JobContext,
  to: string,
  formats: readonly DocumentFormat[],
  _findings: readonly Finding[],
  jlog: Log,
  timed: <T>(name: Stage, work: () => Promise<T>) => Promise<T>,
): Promise<Finding[]> {
  const { jobs } = deps;
  if (!jobs.SEND_EMAIL || deps.redis === undefined) return [];
  await ctx.setState(STAGE.email);
  const attach = formats.at(-1) ?? "docx";
  const result = await timed(STAGE.email, () =>
    sendEmailOnce(
      { jobs, redis: deps.redis as Redis, log: deps.log, metrics: deps.metrics ?? noMetrics, ...(deps.emailTransport ? { transport: deps.emailTransport } : {}) },
      { submissionId: data.submissionId, to, documentKey: data.key, attachments: [{ path: outputPath(jobs.DOCUMENT_OUTPUT_DIR, data.key, attach), mime: MIME[attach] }], requestId: data.requestId },
    ).catch((error: unknown) => ({ outcome: "failed" as const, detail: error instanceof Error ? error.message : String(error) })),
  );
  jlog.info({ outcome: result.outcome }, "email step finished");
  if (result.outcome === "failed") return [{ code: "EmailNotSent", severity: "error", detail: `the mail server did not take the message (${result.detail ?? "unknown error"}); the document is ready` }];
  if (result.outcome === "unknown") return [{ code: "EmailOutcomeUnknown", severity: "error", detail: "the outcome of sending the email is unknown; it was not sent again" }];
  return [];
}

function readRequests(request: Record<string, unknown>): RequestedResource[] {
  const list = request["resource_requests"];
  if (!Array.isArray(list)) return [];
  const out: RequestedResource[] = [];
  for (const r of list) {
    if (r && typeof r === "object") {
      const o = r as Record<string, unknown>;
      if (typeof o["lang_code"] === "string" && typeof o["resource_type"] === "string" && typeof o["book_code"] === "string") out.push({ lang_code: o["lang_code"], resource_type: o["resource_type"], book_code: o["book_code"] });
    }
  }
  return out;
}

interface Resolved {
  request: RequestedResource;
  /** DOC's `resource_lookup_dto` for this request: the repository (url), its directory name, the language as the catalog has it. */
  dto: LookupDto;
  repo: Repo | undefined;
}

/**
 * Catalog lookup, DOC's way (`resource_lookup_dto`, in the core): which repository serves each requested resource. A request the catalog has
 * no repository for is returned as unresolved with a finding, and the document is built from the rest (DOC drops it silently).
 */
async function locate(deps: PipelineDeps, requests: readonly RequestedResource[]): Promise<{ resolved: Resolved[]; unresolved: RequestedResource[]; findings: Finding[] }> {
  const { dtos, repos } = await deps.lookup.resolve(requests, "fixed");
  const resolved: Resolved[] = [];
  const unresolved: RequestedResource[] = [];
  const findings: Finding[] = [];
  requests.forEach((request, i) => {
    const dto = dtos[i];
    if (!dto) {
      unresolved.push(request);
      findings.push({ code: "ResourceNotInCatalog", severity: "warn", lang: request.lang_code, book: request.book_code, detail: `${request.resource_type} has no repository in the catalog for ${request.lang_code}; the document is built without it (DOC drops it too)` });
      return;
    }
    resolved.push({ request, dto, repo: repos.get(dto.url) });
  });
  return { resolved, unresolved, findings };
}

interface Provisioned {
  paths: Map<string, string>;
  findings: Finding[];
  /** Repositories that could not be fetched on the last attempt: their resources are left out and each is a finding. */
  missing: Set<string>;
}

async function provision(deps: PipelineDeps, resolved: readonly Resolved[], signal: AbortSignal, lastAttempt: boolean): Promise<Provisioned> {
  const urls = [...new Set(resolved.map((r) => r.dto.url))];
  const deadlineAt = Date.now() + deps.jobs.STAGE_PROVISION_DEADLINE_MS;
  const result = await deps.acquirer.acquire(urls, deadlineAt);
  if (signal.aborted) throw new JobError("stage_deadline", "provisioning was aborted", { transient: true });
  const findings: Finding[] = [];
  const paths = new Map<string, string>();
  const missing = new Set<string>();
  for (const [url, got] of result.acquired) {
    paths.set(url, got.path);
    if (got.provenance === "stale_served") findings.push({ code: "RepoServedStale", severity: "warn", repo: url, detail: got.refreshError ?? "refresh failed" });
  }
  if (result.failed.length > 0) {
    const first = result.failed[0];
    // Earlier attempts retry (the content host may be briefly down); the last attempt builds the document from what it has: a request ends with a document.
    if (!lastAttempt || result.acquired.size === 0) throw new JobError("resource_unavailable", `could not fetch ${result.failed.map((f) => f.url).join(", ")}: ${first?.reason ?? ""}: ${first?.detail ?? ""}`, { transient: true, detail: { failed: result.failed } });
    for (const f of result.failed) {
      missing.add(f.url);
      findings.push({ code: "RepoNotFetched", severity: "error", repo: f.url, detail: `${f.url}: ${f.reason}: ${f.detail} (after ${f.attempts} attempts); the document is built without it` });
    }
  }
  return { paths, findings, missing };
}

/** The shell's half of the parse call: list and read what the core's readers will look at (a book's subtree of a TN repository; the book's USFM file or its Writer chapter files). */
async function buildParseInput(request: Record<string, unknown>, resolved: readonly Resolved[], provisioned: Provisioned) {
  const dtos: Record<string, unknown>[] = [];
  const tree: Record<string, { files: string[]; texts: Record<string, string | null> }> = {};
  const usfm: Record<string, unknown>[] = [];
  const findings: Finding[] = [];
  for (const r of resolved) {
    const root = provisioned.paths.get(r.dto.url);
    if (root === undefined) continue;
    const langName = r.repo?.language.english_name ?? r.dto.lang_name;
    if (USFM_TYPES.has(r.request.resource_type)) {
      const found = await findUsfm(root, r.dto.dir, r.request.book_code);
      const base = { lang_code: r.request.lang_code, lang_name: langName, book_code: r.request.book_code, resource_type_name: r.dto.resource_type_name };
      if (found.kind === "none") {
        // DOC builds an empty book (no chapters) and goes on; so do we, and say so
        findings.push({ code: "UsfmNotFound", severity: "warn", lang: r.request.lang_code, book: r.request.book_code, repo: r.dto.url, detail: `${r.dto.url} has no USFM file for ${r.request.book_code}; the book is empty in the document` });
        usfm.push({ ...base, source: "" });
      } else if (found.kind === "single") {
        const text = pythonText(await readFile(join(root, found.rel)));
        if (text === null) throw new JobError("core_failure", `${found.rel} is not valid UTF-8`, { transient: false });
        usfm.push({ ...base, source: text });
      } else {
        // a Writer repository: chapter directories with chunk files; the core stitches them into one USFM text
        usfm.push({ ...base, source: "", writer_files: found.files, fallback_title: "" });
      }
    } else {
      dtos.push({ lang_code: r.request.lang_code, lang_name: langName, resource_type: r.request.resource_type, resource_type_name: r.dto.resource_type_name, book_code: r.request.book_code, lang_direction: r.dto.lang_direction, dir: r.dto.dir });
      tree[r.dto.dir] = await listBookTree(root, r.request.book_code, r.request.resource_type);
    }
  }
  // Nothing at all to build a document from (every requested book is absent from its repository): not transient, say so plainly instead of returning an empty document
  const emptyUsfm = usfm.every((u) => u["source"] === "" && u["writer_files"] === undefined);
  if (dtos.length === 0 && usfm.length > 0 && emptyUsfm) {
    throw new JobError("resource_unavailable", `none of the requested resources has content for the requested books: ${findings.map((f) => f.detail).join("; ")}`, { transient: false, detail: { findings } });
  }
  return { request, dtos, tree, usfm, findings };
}

type UsfmFound = { kind: "none" } | { kind: "single"; rel: string } | { kind: "writer"; files: [string, string][] };

/**
 * DOC's `usfm_asset_file`: the `.usfm` files one level below the repository directory, else its `.txt` files, else the `.txt` files two levels below it
 * (Python's glob without `recursive` treats a double star as a single one), `title.txt` excluded; the files of the book are those whose stem (`.usfm`) or whole path
 * (`.txt`, which includes the repository directory's name) names the book code, in any case. One file: that is the book. More than one: a Writer repository,
 * stitched into one USFM by the core.
 */
export async function findUsfm(root: string, dirName: string, book: string): Promise<UsfmFound> {
  const level1 = await filesAt(root, "");
  const level2: string[] = [];
  for (const d of await dirsAt(root)) for (const f of await filesAt(root, d)) level2.push(`${d}/${f}`);
  let candidates = level1.filter((f) => f.endsWith(".usfm"));
  if (candidates.length === 0) candidates = level1.filter((f) => f.endsWith(".txt"));
  if (candidates.length === 0) candidates = level2.filter((f) => f.endsWith(".txt"));
  candidates = candidates.filter((f) => !f.endsWith("title.txt"));
  if (candidates.length === 0) return { kind: "none" };
  const code = book.toLowerCase();
  const stem = (f: string): string => (f.split("/").pop() ?? "").replace(/\.[^.]*$/, "").toLowerCase();
  const chosen = candidates[0]?.endsWith(".usfm") ? candidates.filter((f) => stem(f).includes(code)) : candidates.filter((f) => `${dirName}/${f}`.toLowerCase().includes(code));
  if (chosen.length === 0) return { kind: "none" };
  if (chosen.length === 1) return { kind: "single", rel: chosen[0] as string };
  const files: [string, string][] = [];
  for (const f of [...level1, ...level2].filter((x) => /\.(usfm|txt)$/.test(x)).sort()) {
    const text = pythonText(await readFile(join(root, f)));
    if (text !== null) files.push([f, text]);
  }
  return { kind: "writer", files };
}

async function dirsAt(root: string): Promise<string[]> {
  const entries = await readdir(root, { withFileTypes: true }).catch(() => []);
  return entries.filter((e) => e.isDirectory() && !e.name.startsWith(".")).map((e) => e.name).sort();
}

async function filesAt(root: string, rel: string): Promise<string[]> {
  const entries = await readdir(join(root, rel), { withFileTypes: true }).catch(() => []);
  return entries.filter((e) => e.isFile()).map((e) => e.name).sort();
}

/**
 * What the core's readers will look at, listed and read by the shell (the core does no I/O):
 *  - Translation Notes, Questions, Commentary (`tn`, `tq`, `bc`, `tnc`): every file under `<repo>/<book>/` (the readers' chapter globs reach no further);
 *  - Translation Words (`tw`): the whole dictionary, `bible/kt`, `bible/names` and `bible/other` (a word is looked up from any book's notes).
 * Text of `.md` and `.txt` files travels with the listing.
 */
async function listBookTree(root: string, book: string, resourceType = ""): Promise<{ files: string[]; texts: Record<string, string | null> }> {
  const files: string[] = [];
  const texts: Record<string, string | null> = {};
  async function walk(rel: string): Promise<void> {
    let entries;
    try {
      entries = await readdir(join(root, rel), { withFileTypes: true });
    } catch {
      return;
    }
    entries.sort((a, b) => (a.name < b.name ? -1 : 1));
    for (const e of entries) {
      const p = `${rel}/${e.name}`;
      if (e.isDirectory()) await walk(p);
      else {
        files.push(p);
        if (/\.(md|txt)$/i.test(e.name)) texts[p] = pythonText(await readFile(join(root, p)));
      }
    }
  }
  const roots = resourceType.startsWith("tw") ? ["bible/kt", "bible/names", "bible/other"] : [book, book.toUpperCase()];
  for (const candidate of roots) await walk(candidate);
  return { files, texts };
}
