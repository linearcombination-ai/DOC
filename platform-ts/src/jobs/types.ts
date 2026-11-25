/**
 * The job contract: what goes into the queue, what the status endpoint answers, and DOC's status strings.
 * The strings are matched *literally* by the frontend (`TaskStatus.svelte`, `GenerateDocument.svelte`): never reword.
 */
import { z } from "zod";

/** `state` values DOC's frontend knows, in the order a docx job passes through them (document_generator.py:352-382). */
export const STAGE = {
  receiving: "Receiving request",
  locating: "Locating assets",
  provisioning: "Provisioning asset files",
  parsing: "Parsing asset files",
  assembling: "Assembling content",
  docx: "Converting to Docx",
  /** The two states DOC's other task (`generate_document`) adds. */
  pdf: "Converting to PDF",
  email: "Sending email",
} as const;
export type Stage = (typeof STAGE)[keyof typeof STAGE];
export const STAGE_ORDER: readonly Stage[] = [STAGE.receiving, STAGE.locating, STAGE.provisioning, STAGE.parsing, STAGE.assembling, STAGE.docx, STAGE.pdf, STAGE.email];

export const JobData = z.object({
  /** document_request_key: the cache key, the output file name and the de-duplication id. */
  key: z.string(),
  /** The request exactly as the API received it (the worker re-validates it through the same core). */
  request: z.record(z.string(), z.unknown()),
  /** `docx`: POST /documents_docx (a docx). `document`: POST /documents (the html DOC writes as a by-product, and a PDF when `generate_pdf`). */
  kind: z.enum(["docx", "document"]),
  /** Propagated from the HTTP request (`x-request-id`, or generated there) to every log line of every attempt and of the Python process. */
  requestId: z.string().default(""),
  receivedAt: z.number(),
  /** Unique per submission (not per request): tells a joined duplicate from a new job. */
  submissionId: z.string(),
});
export type JobData = z.infer<typeof JobData>;

/** The failure result: DOC's UI reads only `state`; `result` is the compatible extension. */
export const FailureResult = z.object({ code: z.string(), message: z.string(), retryable: z.boolean() });
export type FailureResult = z.infer<typeof FailureResult>;

/** What `GET /task_status/{id}` answers. Celery's shape: `{state}`, plus `result` on SUCCESS (and, new, on FAILURE). */
export interface TaskStatus {
  state: string;
  result?: string | FailureResult;
  attempt?: number;
}

/** Stored as the BullMQ job progress: the last stage reached (so a retry still shows it) and the attempt. */
export const Progress = z.object({ state: z.string(), attempt: z.number().int() });
export type Progress = z.infer<typeof Progress>;
