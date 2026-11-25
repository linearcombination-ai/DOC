/**
 * The contract: one set of zod schemas from which the shell derives
 *   (a) runtime validation (Hono routes parse requests with them, tests parse responses with them),
 *   (b) the TypeScript types (`z.infer`), exported to the frontend through `generated/api.d.ts`,
 *   (c) the OpenAPI 3.1 document (`GET /openapi.json`, built from the same route objects).
 * Nothing else in the shell declares a request or response shape.
 */
import { z } from "@hono/zod-openapi";

/** `lang_code` as DOC takes it: any non-empty string. In `fixed` mode it is also checked against
 *  `StrictLangCode` and refused with the legacy 422 body (see routes). */
export const LangCode = z.string().min(1).openapi({ example: "pt-br", description: "IETF language code as the catalog spells it (e.g. `sw`, `es-419`, `tca-CO-ticuna`). As built, any string is accepted and an unknown code answers `[]`; `fixed` mode additionally requires `^[A-Za-z0-9-]{1,35}$` (1 to 35 letters, digits and hyphens) and answers 422 otherwise." });
/** The captain's `fixed`-mode bound: 1 to 35 letters, digits and hyphens. */
export const StrictLangCode = z.string().regex(/^[A-Za-z0-9-]{1,35}$/);

export const BookCodes = z.string().min(1).openapi({ example: "gen,exo,mat", description: "Comma-separated book codes, or `all`. As built, any string is accepted. `fixed` mode requires `all` or three-character lowercase codes." });
/** `all`, or three-character lowercase codes separated by commas, at most `max` of them (the captain's default is 100). */
export const strictBookCodes = (max: number) => z.union([z.literal("all"), z.string().regex(/^[a-z0-9]{3}(,[a-z0-9]{3})*$/).refine((v) => v.split(",").length <= max, { message: `at most ${max} book codes` })]);
export const StrictBookCodes = strictBookCodes(100);

export const BookTuple = z.tuple([z.string(), z.string()]).openapi({ description: "[book code, name]" });
export const BookList = z.array(BookTuple);
export const ResourceTypeTuple = z.tuple([z.string(), z.string()]).openapi({ description: "[resource type code, resource type name]" });
export const ResourceTypeList = z.array(ResourceTypeTuple);
export const LanguageTuple = z.tuple([z.string(), z.string(), z.boolean()]).openapi({ description: "[language code, \"national (english)\", is gateway language]" });
export const LanguageList = z.array(LanguageTuple);
export const ChaptersInBooks = z.record(z.string(), z.array(z.number().int()));
/** FastAPI serialises the tuple `({"status": "ok"}, 200)` that the as-built route returns as a two-element array. Kept as is. */
export const QueueHealth = z
  .object({
    redis: z.enum(["up", "down"]),
    waiting: z.number().int().optional(),
    active: z.number().int().optional(),
    delayed: z.number().int().optional(),
    failed: z.number().int().optional(),
    workers: z.number().int().optional(),
    oldestWaitingMs: z.number().nullable().optional(),
    error: z.string().optional(),
  })
  .openapi("QueueHealth");
export const HealthBody = z
  .object({
    status: z.enum(["ok", "degraded"]),
    /** Why the status is not ok, one line each. */
    problems: z.array(z.string()).optional(),
    queue: QueueHealth.optional(),
    redis: z.object({ up: z.boolean(), version: z.string().optional(), latencyMs: z.number().optional(), usedMemory: z.string().optional(), aof: z.boolean().optional(), lastSaveAgeSeconds: z.number().nullable().optional(), evictionPolicy: z.string().optional(), connectedClients: z.number().int().optional() }).optional(),
    workers: z.array(z.object({ id: z.string(), pid: z.number().int(), heartbeatAgeSeconds: z.number(), running: z.number().int(), rssMb: z.number(), docxMode: z.enum(["process", "persistent"]) })).optional(),
    docxPool: z.object({ mode: z.enum(["process", "persistent"]), size: z.number().int(), idle: z.number().int(), busy: z.number().int(), starting: z.number().int(), recycled: z.number().int(), crashed: z.number().int() }).optional(),
    catalog: z.object({ lastSuccessAgeSeconds: z.record(z.string(), z.number()) }).optional(),
    disk: z.object({ freeMb: z.number(), dir: z.string(), floorMb: z.number() }).optional(),
  })
  .openapi("HealthBody");
export const Health = z.tuple([HealthBody, z.literal(200)]);

// --- P5: the document job endpoints
/** The body of POST /documents and /documents_docx. Validation is the core's (DOC's pydantic rules, ported); the contract only says it is an object. */
export const DocumentRequestBody = z.record(z.string(), z.unknown()).openapi("DocumentRequest", { description: "DOC's DocumentRequest; validated by the core with DOC's rules (422 in the legacy body when invalid)." });
export const TaskCreated = z.object({ task_id: z.string() }).openapi("TaskCreated");
export const FailureResult = z.object({ code: z.string(), message: z.string(), retryable: z.boolean() }).openapi("FailureResult");
/**
 * Celery's shape: `{state}`; `result` on SUCCESS (the document request key) and, new and compatible, on FAILURE. Written as a union so a
 * typed client can narrow on `state` (the frontend reads `state` and, when it is `SUCCESS`, `result` as a string; it never reads FAILURE's).
 */
export const TaskStatusInProgress = z
  .object({
    state: z.enum(["PENDING", "Receiving request", "Locating assets", "Provisioning asset files", "Parsing asset files", "Assembling content", "Converting to Docx", "Converting to PDF", "Sending email"]),
    attempt: z.number().int().optional().openapi({ description: "From the second attempt on: which attempt is running (the state stays at the last stage reached)." }),
  })
  .openapi("TaskStatusInProgress");
export const TaskStatusSuccess = z.object({ state: z.literal("SUCCESS"), result: z.string().openapi({ description: "The document request key: the files are `<key>.docx`, `<key>.html`, `<key>.pdf`." }) }).openapi("TaskStatusSuccess");
export const TaskStatusFailure = z.object({ state: z.literal("FAILURE"), result: FailureResult }).openapi("TaskStatusFailure");
export const TaskStatus = z.union([TaskStatusInProgress, TaskStatusSuccess, TaskStatusFailure]).openapi("TaskStatus", { description: "state is one of PENDING, DOC's stage strings, SUCCESS or FAILURE. An unknown task id is PENDING (Celery's behaviour, kept)." });
export const NotImplemented = z.object({ code: z.literal("not_implemented"), message: z.string(), detail: z.string(), retryable: z.literal(false) }).openapi("NotImplemented");
export const RequestTooLarge = z.object({ code: z.literal("request_too_large"), message: z.string(), detail: z.string(), retryable: z.literal(false) }).openapi("RequestTooLarge");
export const DiskLow = z.object({ code: z.literal("disk_space_low"), message: z.string(), detail: z.string(), retryable: z.literal(true) }).openapi("DiskLow");
export const QueueUnavailable = z.object({ code: z.literal("queue_unavailable"), message: z.string(), detail: z.string(), retryable: z.literal(true) }).openapi("QueueUnavailable");

export const Finding = z
  .object({
    code: z.string().openapi({ example: "UnknownBookCode" }),
    severity: z.enum(["info", "warn", "error"]),
    lang: z.string().optional(),
    repo: z.string().optional(),
    book: z.string().optional(),
    detail: z.string(),
  })
  .openapi("Finding");

/** Body when the caller asks for `?findings=1`: the same answer plus what the as-built code swallows. */
export function withFindings<S extends z.ZodType>(result: S) {
  return z.object({
    result,
    findings: z.array(Finding),
    stale: z.boolean().openapi({ description: "The catalog was served from the last good answer because a refresh failed." }),
    failedRepos: z.array(z.object({ url: z.string(), reason: z.string(), detail: z.string(), attempts: z.number().int() })).openapi({ description: "Repositories the request needed and could not fetch." }),
  });
}

export const FindingsQuery = z.object({
  findings: z.enum(["0", "1"]).optional().openapi({ description: "`1` wraps the answer in `{result, findings, stale, failedRepos}`. Absent: the bare DOC-shaped answer, with the finding count in the `x-doc-findings` header." }),
});

/** FastAPI's handler in `app.py` turns RequestValidationError into this body (status 422). Kept for parity. */
export const LegacyValidationError = z
  .object({ status_code: z.literal(10422), message: z.string(), data: z.null() })
  .openapi("LegacyValidationError");

/** What a transient dependency failure looks like in `fixed` mode (HTTP 503 with Retry-After). */
export const ServiceUnavailable = z
  .object({ kind: z.enum(["catalog_unavailable", "deadline_exceeded"]), message: z.string(), transient: z.literal(true), detail: z.record(z.string(), z.unknown()) })
  .openapi("ServiceUnavailable");

export const NotFound = z.object({ detail: z.literal("Not Found") }).openapi("NotFound");
export const MethodNotAllowed = z.object({ detail: z.literal("Method Not Allowed") }).openapi("MethodNotAllowed");

export type BookListT = z.infer<typeof BookList>;
export type LanguageListT = z.infer<typeof LanguageList>;
export type ResourceTypeListT = z.infer<typeof ResourceTypeList>;
export type ChaptersInBooksT = z.infer<typeof ChaptersInBooks>;
