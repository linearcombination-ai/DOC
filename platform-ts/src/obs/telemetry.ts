/** Fleet-wide gauges kept in Redis (worker heartbeats, last good catalog fetch) and the scrape-time collector for `/metrics` and `/health/status`. */
import type { Redis } from "ioredis";
import type { Queue } from "bullmq";
import type { Gauge } from "./metrics.ts";

export interface WorkerBeat {
  id: string;
  pid: number;
  host: string;
  startedAt: number;
  running: number;
  rssBytes: number;
  docxMode: "process" | "persistent";
  pool: { size: number; idle: number; busy: number; starting: number; recycled: number; crashed: number };
  jobsDone: number;
}

export class Telemetry {
  readonly #redis: Redis;
  readonly #prefix: string;
  constructor(redis: Redis, prefix: string) {
    this.#redis = redis;
    this.#prefix = prefix;
  }
  async beat(beat: WorkerBeat, ttlMs: number): Promise<void> {
    await this.#redis.set(`${this.#prefix}:worker:${beat.id}`, JSON.stringify({ ...beat, at: Date.now() }), "PX", ttlMs);
  }
  async retire(id: string): Promise<void> {
    await this.#redis.del(`${this.#prefix}:worker:${id}`);
  }
  async catalogOk(kind: string): Promise<void> {
    await this.#redis.set(`${this.#prefix}:catalog:last:${kind}`, String(Date.now()));
  }
  async workers(): Promise<(WorkerBeat & { at: number })[]> {
    const keys = await this.scan(`${this.#prefix}:worker:*`);
    if (keys.length === 0) return [];
    const values = await this.#redis.mget(keys);
    return values.flatMap((v) => {
      if (!v) return [];
      try {
        return [JSON.parse(v) as WorkerBeat & { at: number }];
      } catch {
        return [];
      }
    });
  }
  async catalogLast(): Promise<Record<string, number>> {
    const out: Record<string, number> = {};
    for (const kind of ["primary", "non_primary"]) {
      const v = await this.#redis.get(`${this.#prefix}:catalog:last:${kind}`);
      if (v) out[kind] = Number(v);
    }
    return out;
  }
  async scan(match: string): Promise<string[]> {
    const found: string[] = [];
    let cursor = "0";
    do {
      const [next, keys] = await this.#redis.scan(cursor, "MATCH", match, "COUNT", 100);
      cursor = next;
      found.push(...keys);
    } while (cursor !== "0");
    return found;
  }
}

export interface FleetView {
  gauges: Gauge[];
  workers: (WorkerBeat & { at: number })[];
  counts: Record<string, number>;
  oldestWaitingMs: number | null;
  catalogLast: Record<string, number>;
}

/** Everything that is a gauge, read now. Throws if Redis is unreachable (the caller reports `doc_redis_up 0`). */
export async function collectFleet(queue: Queue, telemetry: Telemetry, now = Date.now()): Promise<FleetView> {
  const [counts, waiting, workers, catalogLast] = await Promise.all([
    queue.getJobCounts("waiting", "active", "delayed", "failed", "completed"),
    queue.getWaiting(0, 0),
    telemetry.workers(),
    telemetry.catalogLast(),
  ]);
  const oldest = waiting[0]?.timestamp;
  const oldestWaitingMs = oldest === undefined ? null : Math.max(0, now - oldest);
  const gauges: Gauge[] = [];
  for (const [state, n] of Object.entries(counts)) gauges.push({ name: "doc_queue_jobs", labels: { state }, value: n });
  gauges.push({ name: "doc_queue_oldest_waiting_seconds", labels: {}, value: (oldestWaitingMs ?? 0) / 1000 });
  gauges.push({ name: "doc_redis_up", labels: {}, value: 1 });
  gauges.push({ name: "doc_workers_alive", labels: {}, value: workers.length });
  const pool: Record<string, number> = { idle: 0, busy: 0, starting: 0 };
  for (const w of workers) {
    gauges.push({ name: "doc_worker_heartbeat_age_seconds", labels: { worker: w.id }, value: Math.max(0, now - w.at) / 1000 });
    gauges.push({ name: "doc_worker_rss_bytes", labels: { worker: w.id }, value: w.rssBytes });
    gauges.push({ name: "doc_worker_running_jobs", labels: { worker: w.id }, value: w.running });
    pool["idle"] = (pool["idle"] ?? 0) + w.pool.idle;
    pool["busy"] = (pool["busy"] ?? 0) + w.pool.busy;
    pool["starting"] = (pool["starting"] ?? 0) + w.pool.starting;
  }
  for (const [state, n] of Object.entries(pool)) gauges.push({ name: "doc_docx_pool_workers", labels: { state }, value: n });
  for (const [kind, at] of Object.entries(catalogLast)) gauges.push({ name: "doc_catalog_last_success_timestamp_seconds", labels: { kind }, value: Math.round(at / 1000) });
  return { gauges, workers, counts, oldestWaitingMs, catalogLast };
}
