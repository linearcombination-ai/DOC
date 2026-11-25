/** The document-job ops of the core (`job_key`, `job_parse`, `job_assemble`), typed once. Key runs inline (microseconds); parse and assemble run on the core thread. */
import { z } from "zod";
import type { CoreThread } from "./corethread.ts";
import { JobError } from "./errors.ts";

interface WasmModule {
  call(op: string, input: string): string;
}

const Loose = z.record(z.string(), z.unknown());
const Fail = z.object({ ok: z.literal(false), kind: z.string(), error: z.string().optional(), violations: z.array(z.unknown()).optional() });

const KeyOut = z.union([z.object({ ok: z.literal(true), key: z.string(), layout: z.string() }), Fail]);
const ParseOut = z.union([z.object({ ok: z.literal(true), books: Loose, findings: z.array(Loose), title1: z.string(), title2: z.string() }), Fail]);
const AssembleOut = z.union([z.object({ ok: z.literal(true), key: z.string(), layout: z.string(), parts: z.array(Loose), findings: z.array(Loose) }), Fail]);

export type Finding = z.infer<typeof Loose>;
export type RequestKey = { ok: true; key: string; layout: string } | { ok: false; violations: unknown[]; message: string };

export class JobCore {
  readonly #wasm: WasmModule;
  readonly #thread: CoreThread;
  constructor(wasm: WasmModule, thread: CoreThread) {
    this.#wasm = wasm;
    this.#thread = thread;
  }

  /** Validate a request the way DOC's pydantic model does and compute its cache key. */
  key(request: unknown): RequestKey {
    const out = KeyOut.parse(JSON.parse(this.#wasm.call("job_key", JSON.stringify(request))));
    if (out.ok) return out;
    if (out.kind === "violations") return { ok: false, violations: out.violations ?? [], message: JSON.stringify(out.violations ?? []) };
    throw new JobError("core_failure", `core job_key: ${out.error ?? out.kind}`, { transient: false });
  }

  async parse(input: unknown, signal: AbortSignal): Promise<{ books: Record<string, unknown>; findings: Finding[]; title1: string; title2: string }> {
    const out = ParseOut.parse(JSON.parse(await this.#thread.call("job_parse", JSON.stringify(input), signal)));
    if (out.ok) return out;
    throw new JobError("core_failure", `a resource could not be read (${out.kind}): ${out.error ?? ""}`, { transient: false, detail: { kind: out.kind } });
  }

  async assemble(input: unknown, signal: AbortSignal): Promise<{ key: string; layout: string; parts: Record<string, unknown>[]; findings: Finding[] }> {
    const out = AssembleOut.parse(JSON.parse(await this.#thread.call("job_assemble", JSON.stringify(input), signal)));
    if (out.ok) return out;
    if (out.kind === "violations") throw new JobError("invalid_request", `the request is not valid: ${JSON.stringify(out.violations ?? [])}`, { transient: false });
    throw new JobError("core_failure", `the interleaver failed (${out.kind}): ${out.error ?? ""}`, { transient: false, detail: { kind: out.kind } });
  }
}
