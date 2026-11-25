import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { CatalogClient, type CatalogClientOptions } from "../src/catalog/client.ts";
import { CatalogUnavailableError, DeadlineExceededError } from "../src/util/errors.ts";
import { silentLogger } from "../src/util/log.ts";
import { startMockDataApi, type MockDataApi } from "./support/mockDataApi.ts";
import { envelope, syntheticRows } from "./support/snapshotFixture.ts";

let api: MockDataApi;
let clock = 1_000_000;
const sleeps: number[] = [];

beforeEach(async () => {
  clock = 1_000_000;
  sleeps.length = 0;
  api = await startMockDataApi({ primary: envelope(syntheticRows()), nonPrimary: envelope([]) });
});
afterEach(async () => {
  await api.close();
});

function client(profile: "as_built" | "fixed", over: Partial<CatalogClientOptions> = {}): CatalogClient {
  return new CatalogClient({
    url: api.url,
    userAgent: "wa-doc",
    requestedWith: "WA-Tool-Doc",
    ttlMs: 180_000,
    timeoutMs: 400,
    maxAttempts: 4,
    backoff: { baseMs: 10, capMs: 80 },
    deadlineMs: 5_000,
    profile,
    log: silentLogger(),
    now: () => clock,
    sleep: (ms) => {
      sleeps.push(ms);
      return Promise.resolve();
    },
    random: () => 0.5,
    ...over,
  });
}

describe("fixed profile: the production behaviour", () => {
  it("data API down: bounded retries, then a typed error that says why; the next request succeeds (self-heal)", async () => {
    api.setMode({ kind: "down" });
    const c = client("fixed");
    const err = await c.rows("primary").catch((e: unknown) => e);
    expect(err).toBeInstanceOf(CatalogUnavailableError);
    expect((err as CatalogUnavailableError).kind).toBe("catalog_unavailable");
    expect((err as CatalogUnavailableError).transient).toBe(true);
    expect((err as CatalogUnavailableError).attempts).toBe(4);
    expect(api.requests().count).toBe(4);
    expect(sleeps).toHaveLength(3); // backoff between attempts, none after the last
    expect(sleeps.every((ms) => ms >= 0 && ms <= 80)).toBe(true);
    api.setMode({ kind: "ok" });
    const got = await c.rows("primary"); // no cached failure: it asks again and succeeds
    expect(got.rows).toHaveLength(syntheticRows().length);
    expect(got.stale).toBe(false);
  });

  it("returns 500 twice then recovers inside one call: the caller sees only the success", async () => {
    api.setMode({ kind: "flaky", failFirst: 2 });
    const got = await client("fixed").rows("primary");
    expect(got.rows).toHaveLength(syntheticRows().length);
    expect(api.requests().count).toBe(3);
  });

  it("slow data API: each attempt times out, the typed error says timeout, and a later fast answer is used", async () => {
    api.setMode({ kind: "slow", delayMs: 1_500 });
    const c = client("fixed", { maxAttempts: 2 });
    const err = (await c.rows("primary").catch((e: unknown) => e)) as CatalogUnavailableError;
    expect(err).toBeInstanceOf(CatalogUnavailableError);
    expect(err.cause_).toBe("timeout");
    api.setMode({ kind: "ok" });
    expect((await c.rows("primary")).rows.length).toBeGreaterThan(0);
  });

  it.each([
    ["malformed", "malformed_json"],
    ["wrongShape", "wrong_shape"],
    ["graphqlError", "graphql_errors"],
  ] as const)("malformed answer (%s) is a typed %s error, retried, never cached", async (kind, cause) => {
    api.setMode({ kind });
    const c = client("fixed", { maxAttempts: 2 });
    const err = (await c.rows("primary").catch((e: unknown) => e)) as CatalogUnavailableError;
    expect(err.cause_).toBe(cause);
    expect(api.requests().count).toBe(2);
    api.setMode({ kind: "ok" });
    expect((await c.rows("primary")).rows.length).toBeGreaterThan(0);
  });

  it("HTTP 4xx-5xx statuses are typed http_status errors", async () => {
    api.setMode({ kind: "http500" });
    const err = (await client("fixed", { maxAttempts: 1 }).rows("primary").catch((e: unknown) => e)) as CatalogUnavailableError;
    expect(err.cause_).toBe("http_status");
    expect(err.detail["status"]).toBe(500);
  });

  it("caches a success for the TTL and refreshes after it", async () => {
    const c = client("fixed");
    await c.rows("primary");
    await c.rows("primary");
    expect(api.requests().count).toBe(1);
    clock += 180_001;
    await c.rows("primary");
    expect(api.requests().count).toBe(2);
  });

  it("serves the last good answer, marked stale, when a refresh fails; recovers when the API does", async () => {
    const c = client("fixed", { maxAttempts: 2 });
    await c.rows("primary");
    clock += 200_000;
    api.setMode({ kind: "down" });
    const stale = await c.rows("primary");
    expect(stale.stale).toBe(true);
    expect(stale.rows).toHaveLength(syntheticRows().length);
    api.setMode({ kind: "ok" });
    const fresh = await c.rows("primary");
    expect(fresh.stale).toBe(false);
  });

  it("single flight: 25 concurrent identical lookups make one request", async () => {
    api.setMode({ kind: "slow", delayMs: 100 });
    const c = client("fixed", { timeoutMs: 2_000 });
    const results = await Promise.all(Array.from({ length: 25 }, () => c.rows("primary")));
    expect(api.requests().count).toBe(1);
    expect(c.joined).toBe(24);
    expect(new Set(results.map((r) => r.rows)).size).toBe(1);
  });

  it("single flight does not remember a failure: concurrent callers share it, the next caller retries", async () => {
    api.setMode({ kind: "http500" });
    const c = client("fixed", { maxAttempts: 1 });
    const settled = await Promise.allSettled(Array.from({ length: 5 }, () => c.rows("primary")));
    expect(settled.every((s) => s.status === "rejected")).toBe(true);
    expect(api.requests().count).toBe(1);
    api.setMode({ kind: "ok" });
    expect((await c.rows("primary")).rows.length).toBeGreaterThan(0);
  });

  it("overall deadline: gives up with DeadlineExceededError even if attempts remain", async () => {
    api.setMode({ kind: "slow", delayMs: 3_000 });
    const c = client("fixed", { timeoutMs: 1_000, deadlineMs: 150, maxAttempts: 10 });
    const err = await c.rows("primary").catch((e: unknown) => e);
    expect(err).toBeInstanceOf(DeadlineExceededError);
  });

  it("sends DOC's headers and its two GraphQL documents", async () => {
    const c = client("fixed");
    await c.rows("primary");
    await c.rows("non_primary");
    expect(api.requests().userAgents).toEqual(["wa-doc", "wa-doc"]);
    expect(api.requests().requestedWith).toEqual(["WA-Tool-Doc", "WA-Tool-Doc"]);
  });
});

describe("as_built profile: reproduces DOC, including the defect", () => {
  it("a failure becomes an empty catalog and is cached for the TTL (the defect the fixed profile removes)", async () => {
    api.setMode({ kind: "http500" });
    const c = client("as_built");
    const first = await c.rows("primary");
    expect(first.rows).toEqual([]);
    expect(first.failure).toBeDefined();
    api.setMode({ kind: "ok" });
    const second = await c.rows("primary"); // still the cached failure
    expect(second.rows).toEqual([]);
    expect(api.requests().count).toBe(1);
    clock += 180_001; // only the TTL heals it
    expect((await c.rows("primary")).rows.length).toBeGreaterThan(0);
  });

  it("makes exactly one attempt (no retry)", async () => {
    api.setMode({ kind: "down" });
    await client("as_built").rows("primary");
    expect(api.requests().count).toBe(1);
  });
});
