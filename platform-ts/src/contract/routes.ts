import { createRoute, z } from "@hono/zod-openapi";
import * as s from "./schemas.ts";

const json = <T extends z.ZodType>(schema: T, description: string) => ({ content: { "application/json": { schema } }, description });
const errors = {
  422: json(s.LegacyValidationError, "Invalid request (the legacy FastAPI handler's body)"),
  503: json(s.ServiceUnavailable, "A dependency (the catalog) is down and nothing can be served (`fixed` mode)"),
};
const tag = ["DOC wave 1-3"];

export const health = createRoute({
  method: "get",
  path: "/health/status",
  tags: tag,
  summary: "Ping-able server endpoint",
  request: { query: z.object({ strict: z.enum(["0", "1"]).optional().openapi({ description: "`1` answers HTTP 503 (same body) when the status is not ok, for a load balancer." }) }) },
  responses: { 200: json(s.Health, "`[{\"status\":\"ok\"},200]` (a FastAPI quirk, kept) with the queue, Redis, workers, docx pool, catalog and disk added when the job queue is configured"), 503: json(s.Health, "Only with `?strict=1` and a status that is not ok") },
});

export const chaptersInBooks = createRoute({
  method: "get",
  path: "/chapters_in_books",
  tags: tag,
  summary: "Chapter numbers of every book, in canonical order",
  responses: { 200: json(s.ChaptersInBooks, "`{book code: [1..N]}`") },
});

export const languageCodesAndNames = createRoute({
  method: "get",
  path: "/language_codes_and_names",
  tags: tag,
  summary: "All languages in the catalog",
  request: { query: s.FindingsQuery },
  responses: { 200: json(z.union([s.LanguageList, s.withFindings(s.LanguageList)]), "Sorted by displayed name"), 503: errors[503], 422: errors[422] },
});

export const bookCodesForLang = createRoute({
  method: "get",
  path: "/book_codes_for_lang/{lang_code}",
  tags: tag,
  summary: "Books available for a language, with localized names",
  request: { params: z.object({ lang_code: s.LangCode }), query: s.FindingsQuery },
  responses: { 200: json(z.union([s.BookList, s.withFindings(s.BookList)]), "Canonical book order"), ...errors },
});

export const bookCodesForLangFromUsfmOnly = createRoute({
  method: "get",
  path: "/book_codes_for_lang_from_usfm_only/{lang_code}",
  tags: tag,
  summary: "As book_codes_for_lang, English names only where USFM gives no name",
  request: { params: z.object({ lang_code: s.LangCode }), query: s.FindingsQuery },
  responses: { 200: json(z.union([s.BookList, s.withFindings(s.BookList)]), "Canonical book order"), ...errors },
});

export const sharedBookCodes = createRoute({
  method: "get",
  path: "/shared_book_codes/{lang0_code}/{lang1_code}",
  tags: tag,
  summary: "Books both languages have, named as in the first language",
  request: { params: z.object({ lang0_code: s.LangCode, lang1_code: s.LangCode }), query: s.FindingsQuery },
  responses: { 200: json(z.union([s.BookList, s.withFindings(s.BookList)]), "Order and names of the first language"), ...errors },
});

export const resourceTypes = createRoute({
  method: "get",
  path: "/resource_types/{lang_code}/{book_codes}",
  tags: tag,
  summary: "Resource types a language offers for the given books",
  request: { params: z.object({ lang_code: s.LangCode, book_codes: s.BookCodes }), query: s.FindingsQuery },
  responses: { 200: json(z.union([s.ResourceTypeList, s.withFindings(s.ResourceTypeList)]), "Sorted by resource type name"), ...errors },
});

const jobTag = ["DOC wave 4-5 (P5 job path)"];
const jobErrors = { 422: errors[422], 413: json(s.RequestTooLarge, "The request body is larger than MAX_BODY_BYTES"), 503: json(z.union([s.QueueUnavailable, s.DiskLow]), "The job queue (Redis) is unreachable, or the disk is nearly full") };

export const postDocumentsDocx = createRoute({
  method: "post",
  path: "/documents_docx",
  tags: jobTag,
  summary: "Enqueue a docx document; identical in-flight requests share one job",
  request: { body: { content: { "application/json": { schema: s.DocumentRequestBody } }, required: true } },
  responses: { 200: json(s.TaskCreated, "`{task_id}`; poll `/task_status/{task_id}`"), ...jobErrors },
});

export const postDocuments = createRoute({
  method: "post",
  path: "/documents",
  tags: jobTag,
  summary: "Enqueue a document: the html DOC writes as a by-product, and a PDF when generate_pdf is true",
  request: { body: { content: { "application/json": { schema: s.DocumentRequestBody } }, required: true } },
  responses: { 200: json(s.TaskCreated, "`{task_id}`; poll `/task_status/{task_id}`; the files are `<key>.html` and, with generate_pdf, `<key>.pdf`"), 501: json(s.NotImplemented, "PDF output is switched off (`PDF_ENGINE=off`)"), ...jobErrors },
});

export const metrics = createRoute({
  method: "get",
  path: "/metrics",
  tags: jobTag,
  summary: "Prometheus text-format metrics for the whole fleet (queue, stages, attempts, failures, clones, catalog, docx pool, workers)",
  responses: { 200: { content: { "text/plain": { schema: z.string() } }, description: "Prometheus exposition format 0.0.4" } },
});

export const taskStatus = createRoute({
  method: "get",
  path: "/task_status/{task_id}",
  tags: jobTag,
  summary: "Task state in Celery's legacy shape",
  request: { params: z.object({ task_id: z.string().min(1) }) },
  responses: { 200: json(s.TaskStatus, "`{state}`; `{state:\"SUCCESS\", result:<document request key>}`; `{state:\"FAILURE\", result:{code,message,retryable}}`"), 503: jobErrors[503] },
});

export const documentFindings = createRoute({
  method: "get",
  path: "/findings/documents/{key}",
  tags: jobTag,
  summary: "What the job found while building a document (defects in the sources, repairs, stale repositories)",
  request: { params: z.object({ key: z.string().regex(/^[A-Za-z0-9_-]{1,120}$/) }) },
  responses: { 200: json(z.array(z.record(z.string(), z.unknown())), "The findings, in the order the pipeline produced them"), 404: json(s.NotFound, "No such document yet") },
});
