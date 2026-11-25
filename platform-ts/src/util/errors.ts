/** Typed errors: a caller can tell a transient condition from a permanent one by `kind`, not by message. */
export type ErrorKind =
  | "catalog_unavailable" // the data API could not be reached or answered with an error, after retries
  | "catalog_malformed" // the data API answered, but not with a catalog
  | "deadline_exceeded" // the per-request deadline passed
  | "core_failure" // the wasm core returned an error for an input the shell built
  | "bad_request"; // the caller's request is not valid

export class DocError extends Error {
  readonly kind: ErrorKind;
  readonly transient: boolean;
  readonly detail: Record<string, unknown>;

  constructor(kind: ErrorKind, message: string, options: { transient: boolean; detail?: Record<string, unknown>; cause?: unknown }) {
    super(message, options.cause === undefined ? undefined : { cause: options.cause });
    this.name = "DocError";
    this.kind = kind;
    this.transient = options.transient;
    this.detail = options.detail ?? {};
  }
}

export type CatalogFailureCause = "network" | "timeout" | "http_status" | "malformed_json" | "wrong_shape" | "graphql_errors";

export class CatalogUnavailableError extends DocError {
  readonly cause_: CatalogFailureCause;
  readonly attempts: number;
  constructor(cause: CatalogFailureCause, attempts: number, message: string, detail: Record<string, unknown> = {}, cause_?: unknown) {
    super("catalog_unavailable", message, { transient: true, detail: { cause, attempts, ...detail }, cause: cause_ });
    this.name = "CatalogUnavailableError";
    this.cause_ = cause;
    this.attempts = attempts;
  }
}

export class DeadlineExceededError extends DocError {
  constructor(what: string, ms: number) {
    super("deadline_exceeded", `${what} did not finish within ${ms} ms`, { transient: true, detail: { what, ms } });
    this.name = "DeadlineExceededError";
  }
}

export function isDocError(e: unknown): e is DocError {
  return e instanceof DocError;
}

export function errorMessage(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}
