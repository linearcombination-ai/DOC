/**
 * Metrics for a fleet of API and worker processes, in the Prometheus text format.
 *
 * Counters and histogram buckets live in one Redis hash (`<prefix>:metrics`, field `<name>|k=v,k=v`), incremented with
 * HINCRBYFLOAT by whichever process observed the event, so any API process can answer `/metrics` for the whole fleet
 * without a push gateway. The price: the numbers reset if Redis loses its data (Prometheus treats that as a counter reset)
 * and every increment is one Redis command (pipelined, fire and forget, never allowed to fail a job: a dropped increment is
 * itself counted in memory and shown as `doc_metrics_dropped_total`).
 *
 * Gauges are not stored: queue depth, oldest waiting age, worker liveness and the last successful catalog fetch are read
 * at scrape time (see `collect`).
 */
import type { Redis } from "ioredis";

export type Labels = Readonly<Record<string, string>>;

/** Bucket upper bounds in seconds for every duration histogram. */
export const DURATION_BUCKETS: readonly number[] = [0.01, 0.05, 0.1, 0.25, 0.5, 1, 2.5, 5, 10, 30, 60, 120, 300, 900];

export interface Metrics {
  inc(name: string, labels?: Labels, by?: number): void;
  observe(name: string, labels: Labels, seconds: number): void;
}

/** A sink that drops everything (unit tests, tools). */
export const noMetrics: Metrics = { inc: () => undefined, observe: () => undefined };

/** In-memory metrics, same API (tests assert on what a component recorded). */
export class MemoryMetrics implements Metrics {
  readonly fields = new Map<string, number>();
  inc(name: string, labels: Labels = {}, by = 1): void {
    const f = fieldOf(name, labels);
    this.fields.set(f, (this.fields.get(f) ?? 0) + by);
  }
  observe(name: string, labels: Labels, seconds: number): void {
    for (const [f, v] of histogramFields(name, labels, seconds)) this.fields.set(f, (this.fields.get(f) ?? 0) + v);
  }
  value(name: string, labels: Labels = {}): number {
    return this.fields.get(fieldOf(name, labels)) ?? 0;
  }
}

export function fieldOf(name: string, labels: Labels): string {
  const parts = Object.keys(labels).sort().map((k) => `${k}=${(labels[k] ?? "").replace(/[|,=\n"\\]/g, "_")}`);
  return `${name}|${parts.join(",")}`;
}

function histogramFields(name: string, labels: Labels, seconds: number): [string, number][] {
  const out: [string, number][] = [];
  for (const le of DURATION_BUCKETS) if (seconds <= le) out.push([fieldOf(`${name}_bucket`, { ...labels, le: String(le) }), 1]);
  out.push([fieldOf(`${name}_bucket`, { ...labels, le: "+Inf" }), 1]);
  out.push([fieldOf(`${name}_sum`, labels), seconds]);
  out.push([fieldOf(`${name}_count`, labels), 1]);
  return out;
}

export class RedisMetrics implements Metrics {
  readonly #redis: Redis;
  readonly #key: string;
  dropped = 0;
  constructor(redis: Redis, prefix: string) {
    this.#redis = redis;
    this.#key = `${prefix}:metrics`;
  }
  #send(entries: readonly [string, number][]): void {
    const pipeline = this.#redis.pipeline();
    for (const [field, by] of entries) pipeline.hincrbyfloat(this.#key, field, by);
    pipeline.exec().then(
      (results) => {
        if (results?.some(([err]) => err)) this.dropped += 1;
      },
      () => {
        this.dropped += 1;
      },
    );
  }
  inc(name: string, labels: Labels = {}, by = 1): void {
    this.#send([[fieldOf(name, labels), by]]);
  }
  observe(name: string, labels: Labels, seconds: number): void {
    this.#send(histogramFields(name, labels, seconds));
  }
  async snapshot(): Promise<Map<string, number>> {
    const raw = await this.#redis.hgetall(this.#key);
    return new Map(Object.entries(raw).map(([k, v]) => [k, Number(v)]));
  }
}

interface Parsed {
  name: string;
  labels: Record<string, string>;
  value: number;
}

function parseField(field: string, value: number): Parsed {
  const bar = field.indexOf("|");
  const name = bar < 0 ? field : field.slice(0, bar);
  const labels: Record<string, string> = {};
  const rest = bar < 0 ? "" : field.slice(bar + 1);
  if (rest) for (const pair of rest.split(",")) {
    const eq = pair.indexOf("=");
    if (eq > 0) labels[pair.slice(0, eq)] = pair.slice(eq + 1);
  }
  return { name, labels, value };
}

const escapeLabel = (v: string): string => v.replace(/\\/g, "\\\\").replace(/\n/g, "\\n").replace(/"/g, '\\"');
function labelText(labels: Record<string, string>): string {
  const keys = Object.keys(labels);
  if (keys.length === 0) return "";
  return `{${keys.map((k) => `${k}="${escapeLabel(labels[k] ?? "")}"`).join(",")}}`;
}

export const HELP: Readonly<Record<string, { type: "counter" | "gauge" | "histogram"; help: string }>> = {
  doc_job_attempts_total: { type: "counter", help: "Job attempts by result (ok, retry, failed, cache_hit)." },
  doc_job_failures_total: { type: "counter", help: "Failed job attempts by error code and whether the attempt was retried." },
  doc_stage_duration_seconds: { type: "histogram", help: "Duration of each pipeline stage." },
  doc_job_duration_seconds: { type: "histogram", help: "Duration of one whole job attempt." },
  doc_stalled_recoveries_total: { type: "counter", help: "Jobs recovered after their worker stopped renewing the lock." },
  doc_repo_clones_total: { type: "counter", help: "Repository acquisitions by outcome (cloned, failed, joined_other_worker, lock_unavailable)." },
  doc_repo_cache_hits_total: { type: "counter", help: "Repository requests served from the asset cache." },
  doc_catalog_refreshes_total: { type: "counter", help: "Catalog fetches by kind and result (ok, failed, stale_served)." },
  doc_docx_worker_events_total: { type: "counter", help: "Docx worker process events (spawn, crash, timeout, recycled, memory_kill)." },
  doc_docx_jobs_total: { type: "counter", help: "Docx jobs by mode (process, persistent) and result." },
  doc_email_total: { type: "counter", help: "Email step outcomes (sent, duplicate_skipped, failed, unknown)." },
  doc_http_requests_total: { type: "counter", help: "HTTP requests by route and status class (this process only)." },
  doc_queue_jobs: { type: "gauge", help: "Jobs in the queue by state." },
  doc_queue_oldest_waiting_seconds: { type: "gauge", help: "Age of the oldest waiting job (0 when none)." },
  doc_redis_up: { type: "gauge", help: "1 when the API can reach Redis." },
  doc_workers_alive: { type: "gauge", help: "Workers whose heartbeat is fresh." },
  doc_worker_heartbeat_age_seconds: { type: "gauge", help: "Seconds since each worker's last heartbeat." },
  doc_worker_rss_bytes: { type: "gauge", help: "Resident memory of each worker process." },
  doc_worker_running_jobs: { type: "gauge", help: "Jobs each worker is running now." },
  doc_docx_pool_workers: { type: "gauge", help: "Persistent docx workers per state across the fleet." },
  doc_catalog_last_success_timestamp_seconds: { type: "gauge", help: "Unix time of the last successful catalog fetch by kind." },
  doc_metrics_dropped_total: { type: "counter", help: "Metric increments this process could not send to Redis." },
};

export interface Gauge {
  name: string;
  labels: Record<string, string>;
  value: number;
}

/** Render counters (from the hash) and gauges in the Prometheus text format, grouped by metric family with HELP and TYPE. */
export function renderPrometheus(counters: ReadonlyMap<string, number>, gauges: readonly Gauge[]): string {
  const families = new Map<string, Parsed[]>();
  const add = (p: Parsed): void => {
    const family = p.name.replace(/_(bucket|sum|count)$/, "");
    const info = HELP[p.name] ?? HELP[family];
    const key = info?.type === "histogram" ? family : p.name;
    const list = families.get(key);
    if (list) list.push(p);
    else families.set(key, [p]);
  };
  for (const [f, v] of counters) add(parseField(f, v));
  for (const g of gauges) add({ name: g.name, labels: g.labels, value: g.value });
  const lines: string[] = [];
  for (const [family, samples] of [...families].sort(([a], [b]) => (a < b ? -1 : 1))) {
    const info = HELP[family];
    if (info) {
      lines.push(`# HELP ${family} ${info.help}`);
      lines.push(`# TYPE ${family} ${info.type}`);
    }
    const order = (p: Parsed): string => {
      const { le, ...rest } = p.labels;
      const bound = le === undefined ? "" : le === "+Inf" ? "9999999999" : Number(le).toFixed(3).padStart(12, "0");
      return `${labelText(rest)}|${p.name.endsWith("_bucket") ? "0" : p.name.endsWith("_sum") ? "1" : "2"}|${bound}`;
    };
    samples.sort((a, b) => (order(a) < order(b) ? -1 : 1));
    for (const s of samples) lines.push(`${s.name}${labelText(s.labels)} ${Number.isInteger(s.value) ? s.value : Number(s.value.toFixed(6))}`);
  }
  return `${lines.join("\n")}\n`;
}
