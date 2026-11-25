/**
 * The seam to the Rust core, typed once. Every payload that crosses is described here as a zod
 * schema (runtime check of what comes back out of the wasm) and the TypeScript type is inferred
 * from it, so the Rust serde structs and these schemas are the two places a seam change shows up.
 */
import { z } from "zod";

export const Flags = z.object({
  skip_unknown_book_codes: z.boolean(),
  owner_prefix_in_list_path: z.boolean(),
  tolerate_repo_errors: z.boolean(),
  skip_bad_catalog_rows: z.boolean(),
  tolerate_empty_nonprimary: z.boolean(),
  kitchen_names: z.boolean(),
  augment_leak: z.boolean(),
});
export type Flags = z.infer<typeof Flags>;

export const FlagPresets = z.object({ as_built: Flags, fixed: Flags });

export const Language = z.object({ english_name: z.string(), ietf_code: z.string(), national_name: z.string(), direction: z.string() });
export const Repo = z.object({ repo_url: z.string(), resource_type: z.string(), language: Language });
export type Repo = z.infer<typeof Repo>;

export const Finding = z.object({
  code: z.string(),
  severity: z.enum(["info", "warn", "error"]),
  lang: z.string().optional(),
  repo: z.string().optional(),
  book: z.string().optional(),
  detail: z.string(),
});
export type Finding = z.infer<typeof Finding>;

export const Located = z.object({ url: z.string(), dir: z.string(), resource_type: z.string(), fetch: z.boolean() });
export type Located = z.infer<typeof Located>;

export const NormalizeOut = z.object({ repos: z.array(Repo), validation_failure: z.string().nullable(), findings: z.array(Finding) });
export const LangListOut = z.object({ result: z.array(z.tuple([z.string(), z.string(), z.boolean()])), findings: z.array(Finding) });
export const PlanOut = z.object({ repos: z.array(Located), augmented: z.number(), findings: z.array(Finding) });
export const WantedOut = z.object({ reads: z.array(z.string()) });
export const FinishOut = z.object({
  result: z.array(z.tuple([z.string(), z.string()])).nullable(),
  swallowed: z.string().nullable(),
  findings: z.array(Finding),
});
export const ChaptersOut = z.record(z.string(), z.array(z.number().int()));

export interface Listing {
  repos: Record<string, { present: boolean; entries: { p: string; d: boolean }[] }>;
}
export type Texts = Record<string, string | null>;
