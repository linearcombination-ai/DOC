/**
 * The catalog adapter: a client for the BIEL GraphQL data API.
 *
 * Two profiles, one implementation:
 *  - `as_built` reproduces DOC: one attempt, any failure becomes an empty answer, and that empty
 *    answer is cached for the TTL like a success (DOC caches `SourceData(git_repo=[])` for 180 s).
 *  - `fixed` (production behaviour): bounded retries with exponential backoff and full jitter, a
 *    timeout per attempt and a deadline overall, no caching of failures, single flight for concurrent
 *    identical lookups, a short TTL cache for successes, and the last good answer served (marked
 *    stale) when a refresh fails. When nothing can be served the caller gets a typed
 *    `CatalogUnavailableError`, never an empty list pretending to be an answer.
 */
import { z } from "zod";
import { backoffDelayMs, realSleep, type BackoffPolicy, type Sleep } from "../util/backoff.ts";
import { CatalogUnavailableError, DeadlineExceededError, errorMessage, type CatalogFailureCause } from "../util/errors.ts";
import type { Log } from "../util/log.ts";
import type { Metrics } from "../obs/metrics.ts";
import { SingleFlight } from "../util/singleflight.ts";

export type CatalogKind = "primary" | "non_primary";

/** DOC's two GraphQL documents, verbatim (including whitespace: the mock matches on content). */
export const QUERIES: Record<CatalogKind, string> = {
  primary: `
query MyQuery {
  git_repo(
    where: {content: {wa_content_metadata: {status: {_eq: "Primary"}}}}
  ) {
    repo_url
    content {
      resource_type
      language {
        english_name
        ietf_code
        national_name
        direction
      }
    }
  }
}
    `,
  non_primary: `
query MyQuery {
  git_repo(
    where: {content: {language: {ietf_code: {_eq: "bg"}}}}
  ) {
    repo_url
    content {
      resource_type
      language {
        english_name
        ietf_code
        national_name
        direction
      }
    }
  }
}
    `,
};

const Envelope = z.object({ data: z.object({ git_repo: z.array(z.unknown()) }) });
const ErrorsEnvelope = z.object({ errors: z.array(z.unknown()).min(1) });

export interface CatalogRows {
  /** The `data.git_repo` rows exactly as the API sent them (the core decides which survive). */
  rows: readonly unknown[];
  /** Served from the last good answer because a refresh failed. */
  stale: boolean;
  /** `as_built` only: DOC would have produced an empty catalog here and cached it. */
  failure?: string;
}

export interface CatalogClientOptions {
  url: string;
  userAgent: string;
  requestedWith: string;
  ttlMs: number;
  timeoutMs: number;
  maxAttempts: number;
  backoff: BackoffPolicy;
  deadlineMs: number;
  profile: "as_built" | "fixed";
  fetch?: typeof fetch;
  now?: () => number;
  sleep?: Sleep;
  random?: () => number;
  log: Log;
  /** Counters for refreshes and failures; and a callback on every successful fetch (the shell records the time for `/health/status`). */
  metrics?: Metrics;
  onFetched?: (kind: CatalogKind) => void;
}

interface Entry {
  value: CatalogRows;
  at: number;
}

export class CatalogClient {
  readonly #o: CatalogClientOptions;
  readonly #fetch: typeof fetch;
  readonly #now: () => number;
  readonly #sleep: Sleep;
  readonly #random: () => number;
  readonly #log: Log;
  readonly #cache = new Map<CatalogKind, Entry>();
  readonly #lastGood = new Map<CatalogKind, Entry>();
  readonly #flights = new SingleFlight<CatalogRows>();

  constructor(options: CatalogClientOptions) {
    this.#o = options;
    this.#fetch = options.fetch ?? fetch;
    this.#now = options.now ?? Date.now;
    this.#sleep = options.sleep ?? realSleep;
    this.#random = options.random ?? Math.random;
    this.#log = options.log.child({ component: "catalog", profile: options.profile });
  }

  get joined(): number {
    return this.#flights.joined;
  }

  /** Forget what is cached (tests, and an operator's "refresh now"). */
  clear(): void {
    this.#cache.clear();
    this.#lastGood.clear();
  }

  async rows(kind: CatalogKind): Promise<CatalogRows> {
    const cached = this.#cache.get(kind);
    if (cached && this.#now() - cached.at < this.#o.ttlMs) {
      this.#log.debug({ kind, ageMs: this.#now() - cached.at }, "catalog cache hit");
      return cached.value;
    }
    return this.#flights.run(kind, () => this.#refresh(kind));
  }

  async #refresh(kind: CatalogKind): Promise<CatalogRows> {
    const started = this.#now();
    if (this.#o.profile === "as_built") {
      const value = await this.#asBuilt(kind);
      this.#cache.set(kind, { value, at: this.#now() }); // failures too: that is the as-built behaviour
      return value;
    }
    try {
      const rows = await this.#withRetries(kind);
      const value: CatalogRows = { rows, stale: false };
      const entry = { value, at: this.#now() };
      this.#cache.set(kind, entry);
      this.#lastGood.set(kind, entry);
      this.#log.info({ kind, rows: rows.length, ms: this.#now() - started }, "catalog refreshed");
      this.#o.metrics?.inc("doc_catalog_refreshes_total", { kind, result: "ok" });
      this.#o.onFetched?.(kind);
      return value;
    } catch (error) {
      const good = this.#lastGood.get(kind);
      if (good) {
        // Self-heal: keep answering from the last good copy, and try again on the next request
        // (nothing is cached for the failure, so the next call refreshes).
        this.#log.warn({ kind, ageMs: this.#now() - good.at, err: errorMessage(error) }, "catalog refresh failed; serving the last good answer");
        this.#o.metrics?.inc("doc_catalog_refreshes_total", { kind, result: "stale_served" });
        return { rows: good.value.rows, stale: true };
      }
      this.#log.error({ kind, err: errorMessage(error) }, "catalog refresh failed and nothing is cached");
      this.#o.metrics?.inc("doc_catalog_refreshes_total", { kind, result: "failed" });
      throw error;
    }
  }

  /** DOC's `fetch_source_data`: one POST, no timeout, every failure an empty list. */
  async #asBuilt(kind: CatalogKind): Promise<CatalogRows> {
    try {
      const rows = await this.#attempt(kind, 1, AbortSignal.timeout(this.#o.deadlineMs));
      return { rows, stale: false };
    } catch (error) {
      this.#log.warn({ kind, err: errorMessage(error) }, "data API failed; as built this is an empty catalog, cached for the TTL");
      return { rows: [], stale: false, failure: errorMessage(error) };
    }
  }

  async #withRetries(kind: CatalogKind): Promise<readonly unknown[]> {
    const deadline = AbortSignal.timeout(this.#o.deadlineMs);
    let last: CatalogUnavailableError | undefined;
    for (let attempt = 1; attempt <= this.#o.maxAttempts; attempt += 1) {
      try {
        return await this.#attempt(kind, attempt, deadline);
      } catch (error) {
        if (!(error instanceof CatalogUnavailableError)) throw error;
        last = error;
        if (deadline.aborted) break;
        if (attempt === this.#o.maxAttempts) break;
        const wait = Math.max(retryAfterMs(error), backoffDelayMs(this.#o.backoff, attempt - 1, this.#random));
        this.#log.warn({ kind, attempt, of: this.#o.maxAttempts, cause: error.cause_, waitMs: wait, err: error.message }, "data API attempt failed; retrying");
        try {
          await this.#sleep(wait, deadline);
        } catch {
          break;
        }
      }
    }
    if (deadline.aborted) throw new DeadlineExceededError(`catalog lookup (${kind})`, this.#o.deadlineMs);
    throw last ?? new CatalogUnavailableError("network", 0, "no attempt was made");
  }

  async #attempt(kind: CatalogKind, attempt: number, overall: AbortSignal): Promise<readonly unknown[]> {
    const started = this.#now();
    const signal = AbortSignal.any([overall, AbortSignal.timeout(this.#o.timeoutMs)]);
    const fail = (cause: CatalogFailureCause, message: string, detail: Record<string, unknown> = {}, original?: unknown): CatalogUnavailableError =>
      new CatalogUnavailableError(cause, attempt, message, detail, original);
    let response: Response;
    try {
      response = await this.#fetch(this.#o.url, {
        method: "POST",
        headers: { "content-type": "application/json", "user-agent": this.#o.userAgent, "x-requested-with": this.#o.requestedWith },
        body: JSON.stringify({ query: QUERIES[kind] }),
        signal,
      });
    } catch (error) {
      const timedOut = signal.aborted && !overall.aborted;
      throw fail(timedOut ? "timeout" : "network", timedOut ? `no answer within ${this.#o.timeoutMs} ms` : errorMessage(error), {}, error);
    }
    this.#log.debug({ kind, attempt, status: response.status, ms: this.#now() - started }, "data API response");
    if (!response.ok) {
      const retryAfter = response.headers.get("retry-after");
      await response.body?.cancel();
      throw fail("http_status", `data API answered HTTP ${response.status}`, { status: response.status, retryAfter });
    }
    let json: unknown;
    try {
      json = await response.json();
    } catch (error) {
      throw fail("malformed_json", "data API body is not JSON", {}, error);
    }
    const parsed = Envelope.safeParse(json);
    if (!parsed.success) {
      if (ErrorsEnvelope.safeParse(json).success) throw fail("graphql_errors", "data API returned GraphQL errors");
      throw fail("wrong_shape", "data API body has no data.git_repo", { issues: parsed.error.issues.slice(0, 3).map((i) => i.message) });
    }
    return parsed.data.data.git_repo;
  }
}

function retryAfterMs(error: CatalogUnavailableError): number {
  const value = error.detail["retryAfter"];
  const seconds = typeof value === "string" ? Number(value) : Number.NaN;
  return Number.isFinite(seconds) && seconds >= 0 ? Math.min(seconds * 1000, 30_000) : 0;
}
