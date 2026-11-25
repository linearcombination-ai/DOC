/**
 * Compile-time proof that the generated types (`generated/api.d.ts`, from this build's contract) drop into the SvelteKit frontend
 * as it is written today. The frontend is JavaScript-flavoured Svelte (`await res.json()` is `any` there), so each check below
 * restates one call site with the types it WOULD have, taken from the contract, and `tsc --noEmit` (strict) decides. Where the
 * contract and the call site disagree the check says so with `@ts-expect-error` and a comment: those lines are the mismatches the
 * report lists (a mismatch that stops being one makes `tsc` fail on the unused directive).
 *
 * Call sites read: routes/books/+page.svelte:30-36, routes/resource_types/+page.svelte:35, routes/transfer/[repo]/+page.svelte:34-42,
 * routes/settings/GenerateDocument.svelte:44-58 (poll) and :104-120 (generateDocument), TaskStatus.svelte, stet/settings/GenerateDocument.svelte.
 */
import type { components, paths } from "../generated/api.d.ts";

type Body<P extends keyof paths, M extends "get" | "post", S extends number> = paths[P][M] extends { responses: Record<S, { content: { "application/json": infer B } }> } ? B : never;

// ---- the lookup endpoints: the frontend declares Promise<Array<[string, string]>> by hand at each call site
type BookCodes = Extract<Body<"/book_codes_for_lang/{lang_code}", "get", 200>, [string, string][]>;
type Languages = Extract<Body<"/language_codes_and_names", "get", 200>, [string, string, boolean][]>;
type ResourceTypes = Extract<Body<"/resource_types/{lang_code}/{book_codes}", "get", 200>, [string, string][]>;
type Chapters = Body<"/chapters_in_books", "get", 200>;
export const ok1: (x: BookCodes) => Array<[string, string]> = (x) => x;
export const ok3: (x: Languages) => Array<[string, string, boolean]> = (x) => x;
export const ok4: (x: ResourceTypes) => Array<[string, string]> = (x) => x;
export const ok5: (x: Chapters) => Record<string, number[]> = (x) => x;

// ---- GenerateDocument.svelte poll(): `json.state`, and when it is SUCCESS `json.result` as the document key
type TaskStatus = Body<"/task_status/{task_id}", "get", 200>;
export function poll(json: TaskStatus): string | [string, string] {
  if (json.state === "SUCCESS") return [json.state, json.result]; // narrows to the string result: compiles
  return json.state;
}
// the frontend's `$taskStateStore === 'FAILURE'` branch compares a string with the literal: both sides exist in the contract
export const failureIsAState: TaskStatus["state"] = "FAILURE";
// TaskStatus.svelte matches the stage strings literally; every one it knows is a value of the contract's state enum
export const stages: TaskStatus["state"][] = ["Receiving request", "Locating assets", "Provisioning asset files", "Parsing asset files", "Assembling content", "Converting to Docx", "Converting to PDF", "Sending email", "PENDING", "SUCCESS", "FAILURE"];
// @ts-expect-error mismatch 1: a FAILURE result is an object ({code, message, retryable}); a client that assumed `result` is always a string (as the poll code does for SUCCESS) is wrong for FAILURE. The UI never reads it.
export const failureResultIsString: (x: Extract<TaskStatus, { state: "FAILURE" }>) => string = (x) => x.result;

// ---- generateDocument(): POST body and the {task_id} answer
type DocumentRequest = components["schemas"]["DocumentRequest"];
export const request: DocumentRequest = { email_address: null, assembly_strategy_kind: "lbo", layout_for_print: false, generate_pdf: false, generate_epub: false, generate_docx: true, resource_requests: [{ lang_code: "en", resource_type: "ulb", book_code: "jud" }], document_request_source: "ui", limit_words: true, use_prince: false };
type Created = Body<"/documents_docx", "post", 200>;
export const taskId: (x: Created) => string = (x) => x.task_id;

// ---- the error path: `if (!response.ok) { $errorStore = data.detail }`
type Err422 = Body<"/documents_docx", "post", 422>;
type Err413 = Body<"/documents_docx", "post", 413>;
type Err503 = Body<"/documents_docx", "post", 503>;
export const detail413: (x: Err413) => string = (x) => x.detail; // typed errors of this build carry `detail` like FastAPI's HTTPException bodies
export const detail503: (x: Err503) => string = (x) => x.detail;
// @ts-expect-error mismatch 2: the 422 body is the legacy handler's {status_code, message, data} (DOC's own app.py handler); it has no `detail`, so the frontend shows no error text for a rejected request, exactly as it does against FastAPI today
export const detail422: (x: Err422) => string = (x) => x.detail;

// ---- the file server (PUBLIC_FILE_SERVER_URL): the frontend links `${fileServerUrl}/${key}.docx|.pdf|.html|.epub`. This build writes .docx, .html and .pdf and no .epub (removed from DOC); the UI's epub link is dead by construction.
export const extensions = ["docx", "html", "pdf"] as const;
