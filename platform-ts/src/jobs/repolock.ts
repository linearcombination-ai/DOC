/**
 * Cross-process single flight for repository clones: a Redis lock with a lease and a fencing token.
 *
 * Two workers asked for the same repository at the same time used to clone it twice (P5: 4 clones for 2 repositories). Now the
 * first takes `SET <key> <fence>:<token> NX PX lease`; the others poll and, as soon as the holder has published a fresh copy
 * into the (shared) asset directory, use that without cloning (`isDone`).
 *
 *  - Lease: the holder renews every lease/3 (compare-and-pexpire in Lua), so a clone may take longer than the lease; a holder
 *    that dies stops renewing and the lock frees itself after one lease (waiters then clone: self-heal, no stuck lock).
 *  - Fencing: every acquisition draws a number from `INCR <prefix>:fence`, strictly increasing. The number is written into the
 *    generation's metadata; the holder checks `stillHeld()` and the published generation's number before it swaps `current`,
 *    so a worker whose lease expired (paused, partitioned) cannot replace a newer generation with its older one.
 *    Residual window: the check and the rename are two steps (a filesystem has no compare-and-swap); a stale writer that passes
 *    the check and is then paused would still publish. Both generations are complete clones, so the effect is an older but valid
 *    copy until the next refresh, never a torn one.
 *  - Redis unavailable: the work runs without the lock (counted as `lock_unavailable`). A request must end up with a document.
 *  - Scope: the lock name includes a hash of the host and the asset directory, so workers that do not share a directory do not
 *    wait for each other.
 */
import { createHash, randomBytes } from "node:crypto";
import { hostname } from "node:os";
import type { Redis } from "ioredis";
import type { Metrics } from "../obs/metrics.ts";
import type { Log } from "../util/log.ts";

const RENEW = `if redis.call('get', KEYS[1]) == ARGV[1] then return redis.call('pexpire', KEYS[1], ARGV[2]) else return 0 end`;
const RELEASE = `if redis.call('get', KEYS[1]) == ARGV[1] then return redis.call('del', KEYS[1]) else return 0 end`;

export interface LockedWork<T> {
  /** The fencing number of this acquisition (0 when the work runs without the lock). */
  fence: number;
  /** True while this process still holds the lock (always true without it). */
  held: () => Promise<boolean>;
  /** How the work came to run: `locked` (took the lock), `unlocked` (lock unavailable or waited out). */
  mode: "locked" | "unlocked";
  value?: T;
}

export interface RepoLockOptions {
  redis: Redis;
  prefix: string;
  /** Identifies the asset directory on this host (workers sharing it share the lock). */
  scope: string;
  leaseMs: number;
  pollMs: number;
  log: Log;
  metrics: Metrics;
}

export class RepoLock {
  readonly #o: RepoLockOptions;
  readonly #scope: string;
  constructor(options: RepoLockOptions) {
    this.#o = options;
    this.#scope = createHash("sha1").update(`${hostname()}\n${options.scope}`).digest("hex").slice(0, 10);
  }

  /**
   * Run `work` while holding the lock for `name`. While waiting, `isDone` is polled: when it returns a value another worker has
   * produced the result and it is returned without running `work`. After `waitMs` without either, `work` runs unlocked.
   */
  async run<T>(name: string, waitMs: number, isDone: () => Promise<T | undefined>, work: (lock: LockedWork<T>) => Promise<T>): Promise<T> {
    const o = this.#o;
    const key = `${o.prefix}:lock:${this.#scope}:${createHash("sha1").update(name).digest("hex").slice(0, 16)}`;
    const token = randomBytes(8).toString("hex");
    const until = Date.now() + waitMs;
    let fence = 0;
    for (;;) {
      try {
        fence = await o.redis.incr(`${o.prefix}:fence`);
        const got = await o.redis.set(key, `${fence}:${token}`, "PX", o.leaseMs, "NX");
        if (got === "OK") break;
      } catch (error) {
        o.log.warn({ err: error instanceof Error ? error.message : String(error), name }, "clone lock unavailable (Redis); cloning without it");
        o.metrics.inc("doc_repo_clones_total", { outcome: "lock_unavailable" });
        return work({ fence: 0, held: async () => true, mode: "unlocked" });
      }
      const done = await isDone().catch(() => undefined);
      if (done !== undefined) {
        o.metrics.inc("doc_repo_clones_total", { outcome: "joined_other_worker" });
        o.log.info({ name }, "another worker published this repository while we waited; using its copy");
        return done;
      }
      if (Date.now() >= until) {
        o.log.warn({ name, waitedMs: waitMs }, "waited for the clone lock past the limit; cloning without it");
        o.metrics.inc("doc_repo_clones_total", { outcome: "lock_unavailable" });
        return work({ fence: 0, held: async () => true, mode: "unlocked" });
      }
      await new Promise((resolve) => setTimeout(resolve, o.pollMs));
    }
    const mine = `${fence}:${token}`;
    const renewal = setInterval(() => {
      o.redis.eval(RENEW, 1, key, mine, String(o.leaseMs)).catch(() => undefined);
    }, Math.max(50, Math.floor(o.leaseMs / 3)));
    try {
      // double check: the previous holder may have published between our last poll and our acquisition
      const already = await isDone().catch(() => undefined);
      if (already !== undefined) {
        o.metrics.inc("doc_repo_clones_total", { outcome: "joined_other_worker" });
        return already;
      }
      return await work({ fence, held: async () => (await o.redis.get(key).catch(() => mine)) === mine, mode: "locked" });
    } finally {
      clearInterval(renewal);
      await o.redis.eval(RELEASE, 1, key, mine).catch(() => undefined);
    }
  }
}
