import { createRequire } from "node:module";
import { z } from "zod";
import { DocError } from "../util/errors.ts";
import type { Log } from "../util/log.ts";
import * as seam from "./seam.ts";

/** The generated wasm-bindgen module (target nodejs): one function, JSON in and JSON out. */
interface WasmModule {
  call(op: string, input: string): string;
  kitchen_version(): string;
}

const Envelope = z.union([z.object({ ok: z.unknown() }), z.object({ error: z.string() })]);

export interface Core {
  readonly kitchenVersion: string;
  /** Async because it can run on a worker thread (the 21,000-row catalog takes about 300 ms: never on a thread that must stay responsive). */
  normalizeCatalog(rows: readonly unknown[], kind: "primary" | "non_primary", flags: seam.Flags): Promise<z.infer<typeof seam.NormalizeOut>>;
  langCodesAndNames(primary: readonly seam.Repo[], nonPrimary: readonly seam.Repo[], flags: seam.Flags): z.infer<typeof seam.LangListOut>;
  plan(op: "book_codes" | "book_codes_usfm_only" | "resource_types", lang: string, rows: readonly seam.Repo[], flags: seam.Flags, enRgDir: string): z.infer<typeof seam.PlanOut>;
  wanted(lang: string, repos: readonly seam.Located[], listing: seam.Listing, flags: seam.Flags): string[];
  finishBookCodes(args: FinishArgs): z.infer<typeof seam.FinishOut>;
  finishResourceTypes(args: FinishArgs): z.infer<typeof seam.FinishOut>;
  chaptersInBooks(): Record<string, number[]>;
  /** The two flag presets, owned by the core so the shell cannot drift from it. */
  flagPresets(): z.infer<typeof seam.FlagPresets>;
}

export interface FinishArgs {
  lang: string;
  usfmOnly?: boolean;
  books?: string;
  repos: readonly seam.Located[];
  listing: seam.Listing;
  texts?: seam.Texts;
  flags: seam.Flags;
}

export function loadWasm(pkgDir: string): WasmModule {
  const require = createRequire(import.meta.url);
  return require(`${pkgDir}/doc_wasm.js`) as WasmModule;
}

/** Anything that can run one core op off the calling thread (the `CoreThread`). */
export interface OffThread {
  call(op: string, input: string, signal: AbortSignal): Promise<string>;
}

export function createCore(wasm: WasmModule, log: Log, offThread?: OffThread): Core {
  const child = log.child({ component: "core" });
  function unwrap<S extends z.ZodType>(op: string, raw: string, schema: S): z.infer<S> {
    const parsed = Envelope.parse(JSON.parse(raw));
    if ("error" in parsed) throw new DocError("core_failure", `core ${op}: ${parsed.error}`, { transient: false, detail: { op } });
    return schema.parse(parsed.ok);
  }
  function call<S extends z.ZodType>(op: string, input: unknown, schema: S): z.infer<S> {
    const started = performance.now();
    const text = JSON.stringify(input);
    const raw = wasm.call(op, text);
    const parsed = Envelope.parse(JSON.parse(raw));
    child.debug({ op, inBytes: text.length, outBytes: raw.length, ms: Number((performance.now() - started).toFixed(2)) }, "core call");
    if ("error" in parsed) {
      throw new DocError("core_failure", `core ${op}: ${parsed.error}`, { transient: false, detail: { op } });
    }
    return schema.parse(parsed.ok);
  }
  return {
    kitchenVersion: wasm.kitchen_version(),
    normalizeCatalog: async (rows, kind, flags) => {
      if (!offThread) return call("normalize_catalog", { rows, kind, flags }, seam.NormalizeOut);
      const started = performance.now();
      const raw = await offThread.call("normalize_catalog", JSON.stringify({ rows, kind, flags }), AbortSignal.timeout(120_000));
      child.debug({ op: "normalize_catalog", off_thread: true, ms: Number((performance.now() - started).toFixed(1)) }, "core call");
      return unwrap("normalize_catalog", raw, seam.NormalizeOut);
    },
    langCodesAndNames: (primary, non_primary, flags) => call("lang_codes_and_names", { primary, non_primary, flags }, seam.LangListOut),
    plan: (op, lang, rows, flags, en_rg_dir) => call("plan", { op, lang, rows, flags, en_rg_dir }, seam.PlanOut),
    wanted: (lang, repos, listing, flags) => call("wanted", { lang, repos, listing, flags }, seam.WantedOut).reads,
    finishBookCodes: (a) => call("finish_book_codes", { lang: a.lang, usfm_only: a.usfmOnly ?? false, repos: a.repos, listing: a.listing, texts: a.texts ?? {}, flags: a.flags }, seam.FinishOut),
    finishResourceTypes: (a) => call("finish_resource_types", { lang: a.lang, books: a.books ?? "", repos: a.repos, listing: a.listing, flags: a.flags }, seam.FinishOut),
    chaptersInBooks: () => call("chapters_in_books", {}, seam.ChaptersOut),
    flagPresets: () => call("flags", {}, seam.FlagPresets),
  };
}
