/**
 * The docx seam: a versioned `DocxJob` JSON file in, a document file out, produced by Python under the shell's supervision.
 *
 * Two modes behind one interface (`DocxRunner`), switched by `DOCX_MODE`:
 *  - `process`: one Python process per job (`ProcessDocxRunner`, below): nothing survives a job, so nothing can leak between jobs;
 *    costs an interpreter start and the generator's imports on every job.
 *  - `persistent`: a small pool of long-lived Python processes speaking JSON lines (`docxpool.ts`): no start-up per job; needs
 *    the supervision there (deadline kills and replaces, memory ceiling, recycling, restart after a crash).
 * Both kill the whole process group on a deadline and confirm it is gone before they return, so the caller can delete the scratch
 * directory without a writer still running; both re-emit the Python process's JSON log lines in the shell's own log, carrying the
 * request id; both leave validating the output to `validateDocx` (a complete zip, or a non-empty file for html and pdf).
 */
import { spawn } from "node:child_process";
import { readFileSync } from "node:fs";
import { open, stat } from "node:fs/promises";
import { z } from "zod";
import type { Metrics } from "../obs/metrics.ts";
import type { Log } from "../util/log.ts";
import { JobError } from "./errors.ts";

export const DocxPart = z.object({ content: z.string() }).passthrough();
export const DocumentFormat = z.enum(["docx", "html", "pdf"]);
export type DocumentFormat = z.infer<typeof DocumentFormat>;

/** DocxJob contract version 2 (version 1 files are still accepted by the Python side): adds the output `format` and the `request_id` that ties the Python log lines to the HTTP request. */
export const DocxJob = z.object({
  version: z.literal(2),
  format: DocumentFormat,
  request_id: z.string(),
  job_id: z.string(),
  request_key: z.string(),
  parts: z.array(DocxPart),
  layout_for_print: z.boolean(),
  title1: z.string(),
  title2: z.string(),
  book: z.string(),
  output_path: z.string(),
});
export type DocxJob = z.infer<typeof DocxJob>;

export const DocxResult = z.discriminatedUnion("ok", [
  z.object({ ok: z.literal(true), bytes: z.number().int(), ms: z.number(), max_rss_kb: z.number(), rss_kb: z.number().optional(), python: z.string(), jobs: z.number().int().optional() }),
  z.object({ ok: z.literal(false), kind: z.enum(["input", "render", "unavailable"]), message: z.string() }),
]);
export type DocxResult = z.infer<typeof DocxResult>;
export type DocxOk = Extract<DocxResult, { ok: true }>;

export interface DocxRunInput {
  jobFile: string;
  deadlineMs: number;
  /** The stage's or job's abort signal: aborting kills the Python process. */
  signal: AbortSignal;
  requestId: string;
  /** Persistent mode: memory ceiling for this job (a PDF of a whole book needs far more than a docx); default is the pool's. */
  memoryCeilingMb?: number;
}

export interface DocxRunReport {
  result: DocxOk;
  wallMs: number;
  pid: number;
}

export interface PoolStats {
  mode: "process" | "persistent";
  size: number;
  idle: number;
  busy: number;
  starting: number;
  /** Counters since the process started. */
  jobs: number;
  spawned: number;
  crashed: number;
  killed: number;
  recycled: number;
}

export interface DocxRunner {
  readonly mode: "process" | "persistent";
  run(input: DocxRunInput): Promise<DocxRunReport>;
  stats(): PoolStats;
  /** Start workers ahead of the first job (persistent mode); a no-op otherwise. */
  warm(): Promise<void>;
  close(): Promise<void>;
}

/** Re-emit the JSON log lines a Python process wrote to stderr in the shell's log (same stream, same request id); anything else is kept as a plain warning. */
export function relogPython(text: string, log: Log, pid: number): string[] {
  const kept: string[] = [];
  for (const line of text.split("\n")) {
    if (!line.trim()) continue;
    kept.push(line);
    try {
      const record = JSON.parse(line) as Record<string, unknown>;
      const { level, msg, ts: _ts, pid: _pid, ...rest } = record;
      const method = level === "error" ? "error" : level === "warn" ? "warn" : "info";
      log[method]({ ...rest, pyPid: pid }, `python: ${String(msg ?? "")}`);
    } catch {
      log.warn({ pyPid: pid, line: line.slice(0, 300) }, "python wrote a line that is not a log record");
    }
  }
  return kept;
}

export function parseResult(line: string): DocxResult | undefined {
  try {
    const r = DocxResult.safeParse(JSON.parse(line));
    return r.success ? r.data : undefined;
  } catch {
    return undefined;
  }
}

/** Typed errors shared by both modes. */
export function failureFor(parsed: DocxResult | undefined, extra: { pid: number; code?: number | null; signal?: string | null; stderr?: string }): JobError | undefined {
  if (parsed?.ok === false && parsed.kind === "input") return new JobError("docx_job_rejected", `the docx worker rejected the job: ${parsed.message}`, { transient: false, detail: { pid: extra.pid } });
  if (parsed?.ok === false && parsed.kind === "unavailable") return new JobError("pdf_unavailable", parsed.message, { transient: false, detail: { pid: extra.pid } });
  if (parsed?.ok === false) return new JobError("docx_render_failed", `the document generator failed: ${parsed.message}`, { transient: true, detail: { pid: extra.pid, code: extra.code ?? null } });
  return undefined;
}

export interface ProcessRunnerOptions {
  pythonBin: string;
  script: string;
  fault: string;
  log: Log;
  metrics: Metrics;
}

/** One Python process per job. */
export class ProcessDocxRunner implements DocxRunner {
  readonly mode = "process" as const;
  readonly #o: ProcessRunnerOptions;
  #jobs = 0;
  #spawned = 0;
  #crashed = 0;
  #killed = 0;
  #running = 0;
  constructor(options: ProcessRunnerOptions) {
    this.#o = options;
  }
  stats(): PoolStats {
    return { mode: "process", size: 0, idle: 0, busy: this.#running, starting: 0, jobs: this.#jobs, spawned: this.#spawned, crashed: this.#crashed, killed: this.#killed, recycled: 0 };
  }
  async warm(): Promise<void> {}
  async close(): Promise<void> {}

  async run(i: DocxRunInput): Promise<DocxRunReport> {
    const started = performance.now();
    const o = this.#o;
    const env: NodeJS.ProcessEnv = { ...process.env, PYTHONDONTWRITEBYTECODE: "1", PYTHONUNBUFFERED: "1", DOCX_PARENT_PID: String(process.pid) };
    if (o.fault) env["DOCX_WORKER_FAULT"] = o.fault;
    const child = spawn(o.pythonBin, [o.script, i.jobFile], { env, stdio: ["ignore", "pipe", "pipe"], detached: true });
    const pid = child.pid;
    if (pid === undefined) throw new JobError("docx_worker_crashed", "the docx worker could not be started", { transient: true });
    this.#spawned += 1;
    this.#running += 1;
    o.metrics.inc("doc_docx_worker_events_total", { event: "spawn", mode: "process" });
    let stdout = "";
    let stderr = "";
    child.stdout.on("data", (chunk: Buffer) => {
      if (stdout.length < 65_536) stdout += chunk.toString("utf8");
    });
    child.stderr.on("data", (chunk: Buffer) => {
      if (stderr.length < 65_536) stderr += chunk.toString("utf8");
    });
    const exit = new Promise<{ code: number | null; signal: NodeJS.Signals | null }>((resolve) => {
      child.on("error", () => resolve({ code: null, signal: null }));
      child.on("close", (code, signal) => resolve({ code, signal }));
    });
    const stop = AbortSignal.any([i.signal, AbortSignal.timeout(i.deadlineMs)]);
    let killedBy: "deadline" | "abort" | null = null;
    const onStop = (): void => {
      killedBy = i.signal.aborted ? "abort" : "deadline";
      killGroup(pid);
    };
    if (stop.aborted) onStop();
    else stop.addEventListener("abort", onStop, { once: true });
    const { code, signal } = await exit;
    stop.removeEventListener("abort", onStop);
    await confirmGone(pid);
    this.#running -= 1;
    const wallMs = Math.round(performance.now() - started);
    const pylog = o.log.child({ component: "docx", reqId: i.requestId });
    const kept = relogPython(stderr, pylog, pid);
    if (killedBy !== null) {
      this.#killed += 1;
      o.metrics.inc("doc_docx_worker_events_total", { event: killedBy === "deadline" ? "timeout" : "aborted", mode: "process" });
      o.log.warn({ pid, killedBy, wallMs, reqId: i.requestId }, "docx worker killed");
      throw new JobError(killedBy === "deadline" ? "docx_worker_timeout" : "stage_deadline", `the docx worker was killed after ${wallMs} ms (${killedBy})`, { transient: true, detail: { pid, deadlineMs: i.deadlineMs } });
    }
    const line = stdout.trim().split("\n").at(-1) ?? "";
    const parsed = parseResult(line);
    if (code === 0 && parsed?.ok === true) {
      this.#jobs += 1;
      o.metrics.inc("doc_docx_jobs_total", { mode: "process", result: "ok" });
      return { result: parsed, wallMs, pid };
    }
    const typed = failureFor(parsed, { pid, code, signal });
    if (typed) {
      o.metrics.inc("doc_docx_jobs_total", { mode: "process", result: typed.code });
      throw typed;
    }
    this.#crashed += 1;
    o.metrics.inc("doc_docx_worker_events_total", { event: "crash", mode: "process" });
    o.metrics.inc("doc_docx_jobs_total", { mode: "process", result: "crashed" });
    throw new JobError("docx_worker_crashed", `the docx worker ended without a result (exit ${code ?? "none"}, signal ${signal ?? "none"})`, { transient: true, detail: { pid, code, signal, stderr: kept.slice(-3).join(" | ").slice(-500) } });
  }
}

export function killGroup(pid: number): void {
  try {
    process.kill(-pid, "SIGKILL");
  } catch {
    // already gone
  }
}

/** A killed group is gone once `kill(-pid, 0)` says there is no such process; poll briefly (the kernel reaps asynchronously). */
export async function confirmGone(pid: number): Promise<void> {
  for (let i = 0; i < 100; i += 1) {
    try {
      process.kill(-pid, 0);
    } catch {
      return;
    }
    killGroup(pid);
    await new Promise((resolve) => setTimeout(resolve, 20));
  }
}

/** Alive means running: a zombie (exited, not yet reaped by its parent) is gone for every purpose here (Linux: /proc state `Z`). */
export function pidAlive(pid: number): boolean {
  try {
    process.kill(pid, 0);
  } catch {
    return false;
  }
  try {
    const state = /^\d+ \(.*\) (\S)/.exec(readFileSync(`/proc/${pid}/stat`, "utf8"))?.[1];
    return state !== "Z" && state !== "X";
  } catch {
    return true;
  }
}

/** Resident memory of a process in bytes from `/proc/<pid>/status` (VmRSS), or 0 when it cannot be read. */
export function rssBytes(pid: number): number {
  try {
    const match = /^VmRSS:\s+(\d+) kB/m.exec(readFileSync(`/proc/${pid}/status`, "utf8"));
    return match?.[1] ? Number(match[1]) * 1024 : 0;
  } catch {
    return 0;
  }
}

/** The shell's own check of what Python wrote: a non-empty file; for docx a complete zip with the two parts every docx has; for pdf the PDF header. */
export async function validateDocx(path: string, format: DocumentFormat = "docx"): Promise<void> {
  let size = 0;
  try {
    size = (await stat(path)).size;
  } catch {
    throw new JobError("docx_output_invalid", "the document worker reported success but wrote no file", { transient: true });
  }
  if (format === "html") {
    if (size < 100) throw new JobError("docx_output_invalid", `the html file is only ${size} bytes`, { transient: true });
    return;
  }
  if (format === "pdf") {
    const handle = await open(path, "r");
    try {
      const head = Buffer.alloc(5);
      await handle.read(head, 0, 5, 0);
      if (size < 500 || head.toString("latin1") !== "%PDF-") throw new JobError("docx_output_invalid", "the pdf file does not start with %PDF-", { transient: true });
    } finally {
      await handle.close();
    }
    return;
  }
  if (size < 1_000) throw new JobError("docx_output_invalid", `the docx file is only ${size} bytes`, { transient: true });
  const handle = await open(path, "r");
  try {
    const head = Buffer.alloc(4);
    await handle.read(head, 0, 4, 0);
    const tail = Buffer.alloc(Math.min(size, 65_536));
    await handle.read(tail, 0, tail.length, size - tail.length);
    const zipHead = head.toString("latin1", 0, 2) === "PK";
    const hasEocd = tail.includes(Buffer.from([0x50, 0x4b, 0x05, 0x06]));
    if (!zipHead || !hasEocd) throw new JobError("docx_output_invalid", "the docx file is not a complete zip archive", { transient: true });
    const text = (await handle.readFile()).toString("latin1");
    if (!text.includes("[Content_Types].xml") || !text.includes("word/document.xml")) throw new JobError("docx_output_invalid", "the docx file lacks [Content_Types].xml or word/document.xml", { transient: true });
  } finally {
    await handle.close();
  }
}
