/**
 * The persistent docx worker pool: long-lived Python processes (`python/docx_server.py`) speaking JSON lines.
 *
 * What this supervisor guarantees (each one has a test in `test/docx.pool.test.ts`):
 *  - one job at a time per process; more jobs than processes wait in a FIFO (bounded by the caller's stage deadline);
 *  - a per-job deadline: the process is killed (its whole process group, `SIGKILL`) and replaced, the job fails transient
 *    (`docx_worker_timeout`) and is retried by the queue on a fresh process;
 *  - a crash (exit, signal, closed pipe) mid-job fails that job transient; an idle crash is noticed at once and the slot is refilled;
 *  - a memory ceiling: a process whose resident set passes `memoryCeilingMb` mid-job is killed (`docx_worker_memory`, transient);
 *    one that is over it between jobs is recycled after the job that pushed it over;
 *  - recycling after `recycleAfter` jobs (a bound on slow leaks), by a polite `exit` request and then a kill if it lingers;
 *  - orphan safety: the Python side exits if this process vanishes (`DOCX_PARENT_PID`), and `close()` kills every group.
 */
import { spawn, type ChildProcess } from "node:child_process";
import type { Metrics } from "../obs/metrics.ts";
import type { Log } from "../util/log.ts";
import { confirmGone, failureFor, killGroup, parseResult, relogPython, rssBytes, type DocxOk, type DocxRunInput, type DocxRunner, type DocxRunReport, type PoolStats } from "./docx.ts";
import { JobError } from "./errors.ts";

export interface PoolOptions {
  pythonBin: string;
  script: string;
  size: number;
  recycleAfter: number;
  memoryCeilingMb: number;
  startTimeoutMs: number;
  fault: string;
  log: Log;
  metrics: Metrics;
  /** How often the supervisor samples a busy process's resident memory. */
  memoryPollMs?: number;
}

type Reply = { id?: string } & Record<string, unknown>;

interface Slot {
  readonly serial: number;
  readonly child: ChildProcess;
  readonly pid: number;
  state: "starting" | "idle" | "busy" | "dead";
  jobs: number;
  buffer: string;
  stderr: string;
  waiter: { id: string; resolve: (r: Reply) => void } | undefined;
  ready: Promise<void>;
  closed: Promise<void>;
  /** Why the supervisor ended this process, if it did (as opposed to the process dying by itself). */
  endedBy: "deadline" | "abort" | "memory" | "recycle" | "close" | undefined;
}

export class DocxPool implements DocxRunner {
  readonly mode = "persistent" as const;
  readonly #o: PoolOptions;
  readonly #slots = new Set<Slot>();
  readonly #waiting: (() => void)[] = [];
  #serial = 0;
  #requestId = 0;
  #closing = false;
  readonly #counters = { jobs: 0, spawned: 0, crashed: 0, killed: 0, recycled: 0 };

  constructor(options: PoolOptions) {
    this.#o = options;
  }

  stats(): PoolStats {
    let idle = 0;
    let busy = 0;
    let starting = 0;
    for (const s of this.#slots) {
      if (s.state === "idle") idle += 1;
      else if (s.state === "busy") busy += 1;
      else if (s.state === "starting") starting += 1;
    }
    return { mode: "persistent", size: this.#o.size, idle, busy, starting, ...this.#counters };
  }

  async warm(): Promise<void> {
    const started: Promise<void>[] = [];
    for (let i = this.#slots.size; i < this.#o.size; i += 1) started.push(this.#spawn().ready.catch(() => undefined));
    await Promise.all(started);
  }

  async close(): Promise<void> {
    this.#closing = true;
    const all = [...this.#slots];
    for (const s of all) {
      s.endedBy = "close";
      killGroup(s.pid);
    }
    await Promise.all(all.map((s) => s.closed));
    for (const s of all) await confirmGone(s.pid);
  }

  #spawn(): Slot {
    const o = this.#o;
    const env: NodeJS.ProcessEnv = { ...process.env, PYTHONDONTWRITEBYTECODE: "1", PYTHONUNBUFFERED: "1", DOCX_PARENT_PID: String(process.pid) };
    if (o.fault) env["DOCX_WORKER_FAULT"] = o.fault;
    const child = spawn(o.pythonBin, [o.script], { env, stdio: ["pipe", "pipe", "pipe"], detached: true });
    const pid = child.pid ?? 0;
    const slot: Slot = { serial: (this.#serial += 1), child, pid, state: "starting", jobs: 0, buffer: "", stderr: "", waiter: undefined, ready: Promise.resolve(), closed: Promise.resolve(), endedBy: undefined };
    this.#slots.add(slot);
    this.#counters.spawned += 1;
    o.metrics.inc("doc_docx_worker_events_total", { event: "spawn", mode: "persistent" });
    const slog = o.log.child({ component: "docx", slot: slot.serial, pyPid: pid });
    let onReady: () => void = () => undefined;
    let onNotReady: (e: Error) => void = () => undefined;
    slot.ready = new Promise<void>((resolve, reject) => {
      onReady = resolve;
      onNotReady = reject;
    });
    slot.ready.catch(() => undefined);
    const startTimer = setTimeout(() => {
      if (slot.state === "starting") {
        slog.error({ timeoutMs: o.startTimeoutMs }, "persistent docx worker did not become ready; killing it");
        killGroup(pid);
      }
    }, o.startTimeoutMs);
    startTimer.unref();
    child.stdout?.on("data", (chunk: Buffer) => {
      slot.buffer += chunk.toString("utf8");
      let nl = slot.buffer.indexOf("\n");
      while (nl >= 0) {
        const line = slot.buffer.slice(0, nl);
        slot.buffer = slot.buffer.slice(nl + 1);
        nl = slot.buffer.indexOf("\n");
        this.#onLine(slot, line, onReady, slog);
      }
    });
    child.stderr?.on("data", (chunk: Buffer) => {
      slot.stderr += chunk.toString("utf8");
      const cut = slot.stderr.lastIndexOf("\n");
      if (cut >= 0) {
        relogPython(slot.stderr.slice(0, cut), slog, pid);
        slot.stderr = slot.stderr.slice(cut + 1);
      }
      if (slot.stderr.length > 65_536) slot.stderr = slot.stderr.slice(-8_192);
    });
    child.stdin?.on("error", () => undefined); // a write to a dead process: the close handler reports it
    slot.closed = new Promise<void>((resolve) => {
      child.on("error", () => undefined);
      child.on("close", (code, signal) => {
        clearTimeout(startTimer);
        const wasState = slot.state;
        slot.state = "dead";
        this.#slots.delete(slot);
        if (slot.stderr.trim()) relogPython(slot.stderr, slog, pid);
        const planned = slot.endedBy !== undefined;
        if (!planned) {
          this.#counters.crashed += 1;
          o.metrics.inc("doc_docx_worker_events_total", { event: "crash", mode: "persistent" });
          slog.warn({ code, signal, whileState: wasState }, "persistent docx worker exited on its own");
        }
        onNotReady(new Error(`the persistent docx worker exited before it was ready (code ${code}, signal ${signal})`));
        slot.waiter?.resolve({ id: slot.waiter.id, __closed: true, code, signal });
        slot.waiter = undefined;
        this.#wake();
        resolve();
      });
    });
    return slot;
  }

  #onLine(slot: Slot, line: string, ready: () => void, slog: Log): void {
    if (!line.trim()) return;
    let msg: Reply;
    try {
      msg = JSON.parse(line) as Reply;
    } catch {
      slog.warn({ line: line.slice(0, 200) }, "persistent docx worker wrote a non-JSON line on its protocol channel");
      return;
    }
    if (msg["ready"] === true && slot.state === "starting") {
      slot.state = "idle";
      ready();
      this.#wake();
      return;
    }
    if (slot.waiter && String(msg.id ?? "") === slot.waiter.id) {
      const w = slot.waiter;
      slot.waiter = undefined;
      w.resolve(msg);
    }
  }

  #wake(): void {
    const next = this.#waiting.shift();
    if (next) next();
  }

  /** An idle process, a new one if the pool has room, otherwise wait for one to be free (FIFO). */
  async #acquire(signal: AbortSignal): Promise<Slot> {
    for (;;) {
      if (this.#closing) throw new JobError("docx_worker_crashed", "the docx pool is closing", { transient: true });
      if (signal.aborted) throw new JobError("stage_deadline", "the stage deadline passed while waiting for a docx worker", { transient: true });
      for (const s of this.#slots) {
        if (s.state === "idle") {
          s.state = "busy";
          return s;
        }
      }
      if (this.#slots.size < this.#o.size) {
        const s = this.#spawn();
        try {
          await s.ready;
        } catch (error) {
          killGroup(s.pid);
          throw new JobError("docx_worker_crashed", `a docx worker could not be started: ${error instanceof Error ? error.message : String(error)}`, { transient: true });
        }
        if (s.state === "idle") {
          s.state = "busy";
          return s;
        }
        continue;
      }
      await new Promise<void>((resolve) => {
        const done = (): void => {
          signal.removeEventListener("abort", done);
          resolve();
        };
        this.#waiting.push(done);
        signal.addEventListener("abort", done, { once: true });
      });
    }
  }

  async run(input: DocxRunInput): Promise<DocxRunReport> {
    const o = this.#o;
    const started = performance.now();
    const slot = await this.#acquire(input.signal);
    const id = String((this.#requestId += 1));
    const jlog = o.log.child({ component: "docx", slot: slot.serial, pyPid: slot.pid, reqId: input.requestId });
    const ceilingMb = input.memoryCeilingMb ?? o.memoryCeilingMb;
    const ceiling = ceilingMb * 1024 * 1024;
    let stop: "deadline" | "abort" | "memory" | undefined;
    const outcome = await new Promise<Reply>((resolve) => {
      slot.waiter = { id, resolve };
      const end = (why: "deadline" | "abort" | "memory"): void => {
        if (stop || slot.state === "dead") return;
        stop = why;
        slot.endedBy = why;
        killGroup(slot.pid);
      };
      const deadline = setTimeout(() => end("deadline"), input.deadlineMs);
      const onAbort = (): void => end("abort");
      input.signal.addEventListener("abort", onAbort, { once: true });
      if (input.signal.aborted) onAbort();
      const memory = setInterval(() => {
        if (rssBytes(slot.pid) > ceiling) end("memory");
      }, o.memoryPollMs ?? 250);
      slot.closed.then(() => {
        clearTimeout(deadline);
        clearInterval(memory);
        input.signal.removeEventListener("abort", onAbort);
      });
      const wrapped = slot.waiter.resolve;
      slot.waiter.resolve = (r) => {
        clearTimeout(deadline);
        clearInterval(memory);
        input.signal.removeEventListener("abort", onAbort);
        wrapped(r);
      };
      try {
        slot.child.stdin?.write(`${JSON.stringify({ id, op: "render", job_file: input.jobFile })}\n`);
      } catch {
        killGroup(slot.pid);
      }
    });
    const wallMs = Math.round(performance.now() - started);

    if (stop !== undefined) {
      await slot.closed;
      await confirmGone(slot.pid);
      this.#counters.killed += 1;
      o.metrics.inc("doc_docx_worker_events_total", { event: stop === "deadline" ? "timeout" : stop === "memory" ? "memory_kill" : "aborted", mode: "persistent" });
      o.metrics.inc("doc_docx_jobs_total", { mode: "persistent", result: stop });
      jlog.warn({ stop, wallMs, ceilingMb }, "persistent docx worker killed and replaced");
      void this.#refill();
      if (stop === "memory") throw new JobError("docx_worker_memory", `the docx worker passed its memory ceiling of ${ceilingMb} MB and was replaced`, { transient: true, detail: { pid: slot.pid } });
      throw new JobError(stop === "deadline" ? "docx_worker_timeout" : "stage_deadline", `the docx worker was killed after ${wallMs} ms (${stop})`, { transient: true, detail: { pid: slot.pid, deadlineMs: input.deadlineMs } });
    }
    if (outcome["__closed"] === true) {
      o.metrics.inc("doc_docx_jobs_total", { mode: "persistent", result: "crashed" });
      void this.#refill();
      throw new JobError("docx_worker_crashed", `the docx worker exited during the job (code ${String(outcome["code"])}, signal ${String(outcome["signal"])}); it was replaced`, { transient: true, detail: { pid: slot.pid, code: outcome["code"], signal: outcome["signal"] } });
    }
    const parsed = parseResult(JSON.stringify(outcome));
    const typed = failureFor(parsed, { pid: slot.pid });
    if (typed) {
      // the process is healthy: a rejected or failed job does not end it
      this.#release(slot);
      o.metrics.inc("doc_docx_jobs_total", { mode: "persistent", result: typed.code });
      throw typed;
    }
    if (parsed?.ok !== true) {
      killGroup(slot.pid);
      o.metrics.inc("doc_docx_jobs_total", { mode: "persistent", result: "garbled" });
      throw new JobError("docx_worker_crashed", "the docx worker answered with something that is not a result; it was replaced", { transient: true, detail: { pid: slot.pid } });
    }
    const ok: DocxOk = parsed;
    slot.jobs += 1;
    this.#counters.jobs += 1;
    o.metrics.inc("doc_docx_jobs_total", { mode: "persistent", result: "ok" });
    const overMemory = (ok.rss_kb ?? 0) * 1024 > ceiling;
    if (slot.jobs >= o.recycleAfter || overMemory) {
      jlog.info({ jobs: slot.jobs, rssKb: ok.rss_kb, why: overMemory ? "memory" : "job count" }, "recycling the persistent docx worker");
      this.#counters.recycled += 1;
      o.metrics.inc("doc_docx_worker_events_total", { event: "recycled", mode: "persistent" });
      void this.#recycle(slot);
    } else {
      this.#release(slot);
    }
    return { result: ok, wallMs, pid: slot.pid };
  }

  #release(slot: Slot): void {
    if (slot.state === "busy") slot.state = "idle";
    this.#wake();
  }

  /** Ask politely, then kill; replace it either way. */
  async #recycle(slot: Slot): Promise<void> {
    slot.endedBy = "recycle";
    try {
      slot.child.stdin?.write(`${JSON.stringify({ op: "exit" })}\n`);
    } catch {
      // closed already
    }
    const lingered = setTimeout(() => killGroup(slot.pid), 2_000);
    await slot.closed;
    clearTimeout(lingered);
    await confirmGone(slot.pid);
    await this.#refill();
  }

  /** Keep the pool at its size (starts a replacement; failures are logged, the next job's acquire retries). */
  async #refill(): Promise<void> {
    if (this.#closing) return;
    if (this.#slots.size < this.#o.size && this.#waiting.length === 0) {
      const s = this.#spawn();
      await s.ready.catch((error: unknown) => this.#o.log.warn({ err: error instanceof Error ? error.message : String(error) }, "replacement docx worker did not start"));
    } else {
      this.#wake();
    }
  }
}
