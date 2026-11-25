/**
 * The acquirer: shallow git clones into a repository cache.
 *
 *  - Keyed by URL. Each successful clone becomes an immutable generation directory
 *    `<root>/<key>/gen-<id>`, published by atomically replacing the symlink `<root>/<key>/current`
 *    (rename of a symlink is atomic). Readers resolve `current` once and read from the resolved
 *    generation, so a swap can never show a reader half of the old tree and half of the new one.
 *    Old generations are deleted after a grace period longer than any request deadline.
 *  - Single flight per URL, in process (a Map of promises). No file locks.
 *  - Fresh for `stalenessMs` (DOC's 30-minute window, configurable); a stale copy is refreshed,
 *    and if the refresh fails the stale copy is still served and the failure is reported.
 *  - Clone: `--depth=1 --single-branch`, a timeout, bounded attempts with backoff and jitter, and a
 *    check that the result is a real repository (`rev-parse --verify HEAD`) before it is published.
 *  - The result lists every repository that could not be fetched. Nothing is dropped silently.
 */
import { randomBytes } from "node:crypto";
import { mkdir, readdir, readFile, rename, rm, symlink, writeFile, readlink, stat } from "node:fs/promises";
import { join } from "node:path";
import { createHash } from "node:crypto";
import { backoffDelayMs, realSleep, type BackoffPolicy, type Sleep } from "../util/backoff.ts";
import { errorMessage } from "../util/errors.ts";
import type { Log } from "../util/log.ts";
import { SingleFlight } from "../util/singleflight.ts";
import { noMetrics, type Metrics } from "../obs/metrics.ts";
import type { RepoLock } from "../jobs/repolock.ts";
import { runGit, type GitResult } from "./git.ts";

export interface AcquirerOptions {
  root: string;
  gitBin: string;
  userAgent: string;
  requestedWith: string;
  stalenessMs: number;
  cloneTimeoutMs: number;
  maxAttempts: number;
  backoff: BackoffPolicy;
  concurrency: number;
  gcGraceMs: number;
  log: Log;
  /** Cross-process single flight (a Redis lock); absent: single flight inside this process only. */
  lock?: RepoLock;
  /** How long to wait for another worker's clone before cloning anyway. */
  lockWaitMs?: number;
  metrics?: Metrics;
  now?: () => number;
  sleep?: Sleep;
  random?: () => number;
  env?: NodeJS.ProcessEnv;
}

export type Provenance = "fresh_clone" | "cache_hit" | "refreshed" | "stale_served";

export interface Acquired {
  url: string;
  /** The immutable generation directory to read from. */
  path: string;
  provenance: Provenance;
  ageMs: number;
  attempts: number;
  /** Set when `provenance` is `stale_served`: why the refresh failed. */
  refreshError?: string;
}

export interface AcquireFailure {
  url: string;
  reason: "clone_failed" | "clone_timeout" | "invalid_repository" | "deadline" | "io_error";
  detail: string;
  attempts: number;
}

export interface AcquireResult {
  acquired: Map<string, Acquired>;
  failed: AcquireFailure[];
}

interface Meta {
  url: string;
  fetchedAt: number;
  /** Fencing number of the acquisition that produced this generation (0: unlocked). */
  fence?: number;
}

type CloneOutcome = { ok: true; acquired: Acquired } | { ok: false; failure: AcquireFailure };

export class Acquirer {
  readonly #o: AcquirerOptions;
  readonly #now: () => number;
  readonly #sleep: Sleep;
  readonly #random: () => number;
  readonly #log: Log;
  readonly #flights = new SingleFlight<CloneOutcome>();
  #active = 0;
  readonly #waiting: (() => void)[] = [];
  /** Counters for the tests and the report. */
  readonly stats = { clones: 0, cacheHits: 0, cloneFailures: 0, staleServed: 0, joinedOtherWorker: 0, supersededClones: 0 };
  readonly #metrics: Metrics;

  constructor(options: AcquirerOptions) {
    this.#o = options;
    this.#now = options.now ?? Date.now;
    this.#sleep = options.sleep ?? realSleep;
    this.#random = options.random ?? Math.random;
    this.#log = options.log.child({ component: "acquirer" });
    this.#metrics = options.metrics ?? noMetrics;
  }

  get joined(): number {
    return this.#flights.joined;
  }

  static keyFor(url: string): string {
    const readable = url.replace(/^https?:\/\//, "").replace(/[^A-Za-z0-9._-]+/g, "_").slice(0, 80);
    return `${readable}-${createHash("sha256").update(url).digest("hex").slice(0, 10)}`;
  }

  /** Remove temp directories a crash left behind. Call once at start. */
  async recover(): Promise<void> {
    const tmp = join(this.#o.root, ".tmp");
    await rm(tmp, { recursive: true, force: true });
    await mkdir(tmp, { recursive: true });
  }

  /**
   * Make every repository available, within `deadlineAt` (epoch ms). Repositories are fetched
   * concurrently (bounded); each one shares an in-flight clone with any other request for it.
   */
  async acquire(urls: readonly string[], deadlineAt: number): Promise<AcquireResult> {
    const unique = [...new Set(urls)];
    const outcomes = await Promise.all(unique.map((url) => this.#one(url, deadlineAt)));
    const acquired = new Map<string, Acquired>();
    const failed: AcquireFailure[] = [];
    for (const o of outcomes) {
      if (o.ok) acquired.set(o.acquired.url, o.acquired);
      else failed.push(o.failure);
    }
    return { acquired, failed };
  }

  async #one(url: string, deadlineAt: number): Promise<CloneOutcome> {
    const key = Acquirer.keyFor(url);
    const current = await this.#current(key, url);
    if (current && this.#now() - current.meta.fetchedAt <= this.#o.stalenessMs) {
      this.stats.cacheHits += 1;
      this.#metrics.inc("doc_repo_cache_hits_total");
      this.#log.debug({ url, ageMs: this.#now() - current.meta.fetchedAt }, "repository cache hit");
      return { ok: true, acquired: { url, path: current.path, provenance: "cache_hit", ageMs: this.#now() - current.meta.fetchedAt, attempts: 0 } };
    }
    const shared = this.#flights.run(url, () => this.#cloneAndPublish(url, key, current));
    const remaining = deadlineAt - this.#now();
    if (remaining <= 0) return this.#deadline(url, current);
    // The deadline is this caller's; the clone itself keeps going for whoever asks next (self-heal).
    let timer: NodeJS.Timeout | undefined;
    const timeout = new Promise<"deadline">((resolve) => {
      timer = setTimeout(() => resolve("deadline"), remaining);
    });
    try {
      const winner = await Promise.race([shared, timeout]);
      return winner === "deadline" ? this.#deadline(url, current) : winner;
    } finally {
      clearTimeout(timer);
    }
  }

  #deadline(url: string, current: { path: string; meta: Meta } | undefined): CloneOutcome {
    if (current) {
      this.stats.staleServed += 1;
      return { ok: true, acquired: { url, path: current.path, provenance: "stale_served", ageMs: this.#now() - current.meta.fetchedAt, attempts: 0, refreshError: "request deadline passed while refreshing" } };
    }
    return { ok: false, failure: { url, reason: "deadline", detail: "the request deadline passed before the clone finished (it continues in the background)", attempts: 0 } };
  }

  async #current(key: string, url: string): Promise<{ path: string; meta: Meta } | undefined> {
    const link = join(this.#o.root, key, "current");
    try {
      const target = await readlink(link);
      const path = join(this.#o.root, key, target);
      const meta = JSON.parse(await readFile(join(path, ".doc-meta.json"), "utf8")) as Meta;
      if (meta.url !== url) return undefined;
      await stat(join(path, ".git", "HEAD"));
      return { path, meta };
    } catch {
      return undefined;
    }
  }

  async #slot<T>(work: () => Promise<T>): Promise<T> {
    while (this.#active >= this.#o.concurrency) {
      await new Promise<void>((resolve) => this.#waiting.push(resolve));
    }
    this.#active += 1;
    try {
      return await work();
    } finally {
      this.#active -= 1;
      this.#waiting.shift()?.();
    }
  }

  async #cloneAndPublish(url: string, key: string, previous: { path: string; meta: Meta } | undefined): Promise<CloneOutcome> {
    const lock = this.#o.lock;
    if (!lock) return this.#slot(() => this.#cloneLocked(url, key, previous, 0, async () => true));
    // Another worker may publish while we wait for the lock: that is the outcome we want (no second clone).
    const fresh = async (): Promise<CloneOutcome | undefined> => {
      const current = await this.#current(key, url);
      if (current && this.#now() - current.meta.fetchedAt <= this.#o.stalenessMs && current.path !== previous?.path) {
        this.stats.joinedOtherWorker += 1;
        return { ok: true, acquired: { url, path: current.path, provenance: "cache_hit", ageMs: this.#now() - current.meta.fetchedAt, attempts: 0 } };
      }
      return undefined;
    };
    return lock.run<CloneOutcome>(url, this.#o.lockWaitMs ?? this.#o.cloneTimeoutMs + 30_000, fresh, (held) => this.#slot(() => this.#cloneLocked(url, key, previous, held.fence, held.held)));
  }

  async #cloneLocked(url: string, key: string, previous: { path: string; meta: Meta } | undefined, fence: number, stillHeld: () => Promise<boolean>): Promise<CloneOutcome> {
    return (async () => {
      let attempts = 0;
      let last: AcquireFailure = { url, reason: "clone_failed", detail: "no attempt", attempts: 0 };
      for (let attempt = 1; attempt <= this.#o.maxAttempts; attempt += 1) {
        attempts = attempt;
        const tmp = join(this.#o.root, ".tmp", `${key}.${randomBytes(4).toString("hex")}`);
        const git = await this.#clone(url, tmp);
        if (git.ok) {
          const valid = await this.#validate(tmp);
          if (valid) {
            try {
              const newer = fence > 0 ? await this.#newerGeneration(key, url, fence) : undefined;
              if (newer || (fence > 0 && !(await stillHeld()) && (await this.#current(key, url)))) {
                // fenced out: our lease expired and a later holder already published; keep theirs
                await rm(tmp, { recursive: true, force: true });
                const cur = newer ?? (await this.#current(key, url));
                if (cur) {
                  this.stats.supersededClones += 1;
                  this.#log.warn({ url, fence }, "this clone was fenced out by a newer generation; using that one");
                  return { ok: true, acquired: { url, path: cur.path, provenance: "cache_hit", ageMs: this.#now() - cur.meta.fetchedAt, attempts } };
                }
              }
              const path = await this.#publish(url, key, tmp, previous?.path, fence);
              this.stats.clones += 1;
              this.#metrics.inc("doc_repo_clones_total", { outcome: "cloned" });
              this.#log.info({ url, attempts, ms: git.ms, replaced: previous !== undefined, fence }, "repository cloned");
              return { ok: true, acquired: { url, path, provenance: previous ? "refreshed" : "fresh_clone", ageMs: 0, attempts } };
            } catch (error) {
              last = { url, reason: "io_error", detail: errorMessage(error), attempts };
            }
          } else {
            last = { url, reason: "invalid_repository", detail: "clone finished but HEAD does not resolve (partial clone)", attempts };
          }
        } else {
          last = { url, reason: git.timedOut ? "clone_timeout" : "clone_failed", detail: git.timedOut ? `no completion within ${this.#o.cloneTimeoutMs} ms` : `git exited ${git.code}: ${git.stderr.split("\n").slice(-2).join(" | ")}`, attempts };
        }
        await rm(tmp, { recursive: true, force: true });
        this.#log.warn({ url, attempt, of: this.#o.maxAttempts, reason: last.reason, detail: last.detail }, "clone attempt failed");
        if (attempt < this.#o.maxAttempts) await this.#sleep(backoffDelayMs(this.#o.backoff, attempt - 1, this.#random));
      }
      this.stats.cloneFailures += 1;
      this.#metrics.inc("doc_repo_clones_total", { outcome: "failed" });
      if (previous) {
        this.stats.staleServed += 1;
        this.#log.error({ ...last }, "refresh failed; serving the stale copy");
        return { ok: true, acquired: { url, path: previous.path, provenance: "stale_served", ageMs: this.#now() - previous.meta.fetchedAt, attempts, refreshError: `${last.reason}: ${last.detail}` } };
      }
      this.#log.error({ ...last }, "repository could not be fetched");
      return { ok: false, failure: last };
    })();
  }

  /** The current generation, if its fencing number is higher than ours (a later holder published it). */
  async #newerGeneration(key: string, url: string, fence: number): Promise<{ path: string; meta: Meta } | undefined> {
    const cur = await this.#current(key, url);
    return cur && (cur.meta.fence ?? 0) > fence ? cur : undefined;
  }

  #clone(url: string, dest: string): Promise<GitResult> {
    const args = [
      "-c", `http.userAgent=${this.#o.userAgent}`,
      "-c", `http.extraHeader=X-Requested-With:${this.#o.requestedWith}`,
      "clone", "--depth=1", "--single-branch", "--quiet", url, dest,
    ];
    this.#log.debug({ url, dest }, "git clone");
    return runGit(this.#o.gitBin, args, { timeoutMs: this.#o.cloneTimeoutMs, ...(this.#o.env ? { env: this.#o.env } : {}) });
  }

  async #validate(dir: string): Promise<boolean> {
    // --git-dir, not -C: if `dir/.git` is not a repository git would otherwise walk up and
    // validate against whatever repository encloses the cache directory.
    const r = await runGit(this.#o.gitBin, [`--git-dir=${join(dir, ".git")}`, "rev-parse", "--verify", "--quiet", "HEAD^{commit}"], { timeoutMs: 30_000, ...(this.#o.env ? { env: this.#o.env } : {}) });
    return r.ok;
  }

  /** Move the clone into its generation directory and flip `current` to it atomically. */
  async #publish(url: string, key: string, tmp: string, previousPath: string | undefined, fence: number): Promise<string> {
    const base = join(this.#o.root, key);
    await mkdir(base, { recursive: true });
    const gen = `gen-${this.#now().toString(36)}-${randomBytes(3).toString("hex")}`;
    const dest = join(base, gen);
    await writeFile(join(tmp, ".doc-meta.json"), JSON.stringify({ url, fetchedAt: this.#now(), fence } satisfies Meta));
    await rename(tmp, dest);
    const linkTmp = join(base, `.current.${randomBytes(3).toString("hex")}`);
    await symlink(gen, linkTmp);
    await rename(linkTmp, join(base, "current")); // atomic replace
    if (previousPath) this.#retire(previousPath);
    void this.#collect(base, gen);
    return dest;
  }

  /** Delete a superseded generation once no request can still be reading it. */
  #retire(path: string): void {
    const timer = setTimeout(() => {
      rm(path, { recursive: true, force: true }).catch((error: unknown) => this.#log.warn({ path, err: errorMessage(error) }, "could not remove an old generation"));
    }, this.#o.gcGraceMs);
    timer.unref();
  }

  /** Orphaned generations (a crash between publish and retire) are removed when they are old enough. */
  async #collect(base: string, keep: string): Promise<void> {
    try {
      for (const name of await readdir(base)) {
        if (!name.startsWith("gen-") || name === keep) continue;
        const info = await stat(join(base, name));
        if (this.#now() - info.mtimeMs > this.#o.gcGraceMs) await rm(join(base, name), { recursive: true, force: true });
      }
    } catch {
      // best effort
    }
  }
}
