/**
 * The wasm core on a worker thread, so a core call that never returns can be killed.
 *
 * A synchronous wasm call on the main thread cannot be interrupted: no AbortSignal, no timer and no BullMQ lock renewal
 * runs while it spins (and the job would be declared stalled and run again elsewhere while this process keeps burning a
 * core). On a `worker_threads` thread the host can `terminate()` it at the deadline; the next call starts a fresh thread.
 * Cost: a thread start and a module load (measured in the report) on the first call, then one message round trip per call.
 */
import { Worker } from "node:worker_threads";
import { JobError } from "./errors.ts";

interface Reply {
  id: number;
  ok: boolean;
  output?: string;
  error?: string;
}

export class CoreThread {
  readonly #pkgDir: string;
  readonly #fault: string;
  #worker: Worker | undefined;
  #nextId = 1;
  readonly #pending = new Map<number, { resolve: (s: string) => void; reject: (e: Error) => void }>();
  terminated = 0;

  constructor(pkgDir: string, fault = "") {
    this.#pkgDir = pkgDir;
    this.#fault = fault;
  }

  #start(): Worker {
    const worker = new Worker(new URL("./corethread.entry.ts", import.meta.url), { workerData: { pkgDir: this.#pkgDir, fault: this.#fault } });
    worker.on("message", (m: Reply) => {
      const p = this.#pending.get(m.id);
      if (!p) return;
      this.#pending.delete(m.id);
      if (m.ok && m.output !== undefined) p.resolve(m.output);
      else p.reject(new Error(m.error ?? "core thread failed"));
    });
    const fail = (e: Error): void => {
      for (const p of this.#pending.values()) p.reject(e);
      this.#pending.clear();
      if (this.#worker === worker) this.#worker = undefined;
    };
    worker.on("error", fail);
    worker.on("exit", (code) => fail(new Error(`core thread exited with ${code}`)));
    worker.unref();
    return worker;
  }

  /** Run one core op; if `signal` aborts first the thread is terminated and the call rejects with a transient deadline error. */
  call(op: string, input: string, signal: AbortSignal): Promise<string> {
    if (signal.aborted) return Promise.reject(new JobError("stage_deadline", `core ${op} not started: deadline already passed`, { transient: true }));
    const worker = (this.#worker ??= this.#start());
    const id = this.#nextId++;
    return new Promise<string>((resolve, reject) => {
      const onAbort = (): void => {
        this.#pending.delete(id);
        this.terminated += 1;
        this.#worker = undefined;
        void worker.terminate();
        reject(new JobError("stage_deadline", `core ${op} did not finish before its deadline; the core thread was terminated`, { transient: true, detail: { op } }));
      };
      signal.addEventListener("abort", onAbort, { once: true });
      this.#pending.set(id, {
        resolve: (s) => {
          signal.removeEventListener("abort", onAbort);
          resolve(s);
        },
        reject: (e) => {
          signal.removeEventListener("abort", onAbort);
          reject(e);
        },
      });
      worker.postMessage({ id, op, input });
    });
  }

  async close(): Promise<void> {
    const w = this.#worker;
    this.#worker = undefined;
    if (w) await w.terminate();
  }
}
