import { DocError } from "../util/errors.ts";

/** Why a job attempt failed, for the failure result and the logs. `retryable` is what the *caller* may do: resubmit. */
export type JobErrorCode =
  | "catalog_unavailable"
  | "resource_unavailable"
  | "stage_deadline"
  | "job_deadline"
  | "docx_worker_crashed"
  | "docx_worker_timeout"
  | "docx_worker_memory"
  | "docx_output_invalid"
  | "docx_render_failed"
  | "docx_job_rejected"
  | "core_failure"
  | "invalid_request"
  | "no_resources"
  | "request_key_mismatch"
  | "scratch_io"
  | "disk_space_low"
  | "pdf_unavailable"
  | "job_stalled"
  | "internal";

/**
 * An error with a verdict. `transient` errors are retried with backoff (up to the job's attempts); permanent ones are
 * thrown as BullMQ's UnrecoverableError by the worker so no further attempt is made.
 */
export class JobError extends Error {
  readonly code: JobErrorCode;
  readonly transient: boolean;
  readonly detail: Record<string, unknown>;
  constructor(code: JobErrorCode, message: string, options: { transient: boolean; detail?: Record<string, unknown>; cause?: unknown }) {
    super(message, options.cause === undefined ? undefined : { cause: options.cause });
    this.name = "JobError";
    this.code = code;
    this.transient = options.transient;
    this.detail = options.detail ?? {};
  }
}

/** Classify anything thrown inside a stage. Unknown errors are treated as transient (retry, then fail loudly). */
export function classify(error: unknown): JobError {
  if (error instanceof JobError) return error;
  if (error instanceof DocError) {
    if (error.kind === "catalog_unavailable") return new JobError("catalog_unavailable", error.message, { transient: true, detail: error.detail, cause: error });
    if (error.kind === "deadline_exceeded") return new JobError("stage_deadline", error.message, { transient: true, detail: error.detail, cause: error });
    if (error.kind === "catalog_malformed") return new JobError("catalog_unavailable", error.message, { transient: true, detail: error.detail, cause: error });
    if (error.kind === "core_failure") return new JobError("core_failure", error.message, { transient: false, detail: error.detail, cause: error });
    return new JobError("invalid_request", error.message, { transient: false, detail: error.detail, cause: error });
  }
  const message = error instanceof Error ? error.message : String(error);
  return new JobError("internal", message, { transient: true, cause: error });
}
