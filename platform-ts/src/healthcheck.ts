/** Container health check: `node src/healthcheck.ts <url>` exits 0 when the URL answers 2xx within the timeout, else 1 (the slim images have no curl). */
const url = process.argv[2] ?? `http://127.0.0.1:${process.env["PORT"] ?? "18301"}/health/status`;
const timeoutMs = Number(process.env["HEALTHCHECK_TIMEOUT_MS"] ?? "4000");
try {
  const res = await fetch(url, { signal: AbortSignal.timeout(timeoutMs) });
  process.exit(res.ok ? 0 : 1);
} catch {
  process.exit(1);
}
