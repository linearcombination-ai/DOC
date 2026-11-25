/** One place that turns the validated configuration into ioredis options: URL, credentials, TLS. Queue, worker, locks, metrics and health all use it. */
import { readFileSync } from "node:fs";
import { Redis, type RedisOptions } from "ioredis";
import type { JobConfig } from "./config.ts";

export function redisOptions(jobs: JobConfig, extra: RedisOptions = {}): RedisOptions {
  const url = new URL(jobs.REDIS_URL);
  const tls = jobs.REDIS_TLS || url.protocol === "rediss:";
  const options: RedisOptions = {
    connectTimeout: jobs.REDIS_CONNECT_TIMEOUT_MS,
    ...(jobs.REDIS_USERNAME !== "" ? { username: jobs.REDIS_USERNAME } : {}),
    ...(jobs.REDIS_PASSWORD !== "" ? { password: jobs.REDIS_PASSWORD } : {}),
    ...extra,
  };
  if (tls) {
    options.tls = {
      ...(jobs.REDIS_TLS_CA !== "" ? { ca: readFileSync(jobs.REDIS_TLS_CA) } : {}),
      ...(jobs.REDIS_TLS_CERT !== "" ? { cert: readFileSync(jobs.REDIS_TLS_CERT), key: readFileSync(jobs.REDIS_TLS_KEY) } : {}),
      ...(jobs.REDIS_TLS_SERVERNAME !== "" ? { servername: jobs.REDIS_TLS_SERVERNAME } : {}),
      rejectUnauthorized: jobs.REDIS_TLS_REJECT_UNAUTHORIZED,
    };
  }
  return options;
}

/** A connection from the configuration. The URL's own credentials and database apply; explicit settings win. */
export function connectRedis(jobs: JobConfig, extra: RedisOptions = {}): Redis {
  // ioredis 6's constructor overloads reject a value typed as its own exported `RedisOptions` (it accepts the same fields as a literal);
  // the cast to the empty object type passes the overload check without widening anything the options builder returns.
  return new Redis(jobs.REDIS_URL, redisOptions(jobs, extra) as object);
}

/** What an operator needs to see about the connection without a secret in it. */
export function describeRedis(jobs: JobConfig): { host: string; port: string; db: string; tls: boolean; auth: boolean; verify: boolean } {
  const url = new URL(jobs.REDIS_URL);
  return {
    host: url.hostname,
    port: url.port || "6379",
    db: url.pathname.replace("/", "") || "0",
    tls: jobs.REDIS_TLS || url.protocol === "rediss:",
    auth: jobs.REDIS_PASSWORD !== "" || url.password !== "",
    verify: jobs.REDIS_TLS_REJECT_UNAUTHORIZED,
  };
}
