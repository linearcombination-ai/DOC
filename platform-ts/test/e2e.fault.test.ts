/** Fault injection through the whole HTTP stack: the data API and git misbehave, the responses are checked. */
import { execFileSync } from "node:child_process";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { buildApp } from "../src/main.ts";
import { loadConfig } from "../src/config.ts";
import { silentLogger } from "../src/util/log.ts";
import { createFixtures, HOST, makeFlakyGit, type Fixtures } from "./support/gitFixtures.ts";
import { startMockDataApi, type MockDataApi } from "./support/mockDataApi.ts";
import { envelope } from "./support/snapshotFixture.ts";

let fx: Fixtures;
let api: MockDataApi;
let flaky: ReturnType<typeof makeFlakyGit>;
let cache: string;

const SW = { english_name: "Swahili", ietf_code: "sw", national_name: "Kiswahili", direction: "ltr" };
const row = (path: string, type: string) => ({ repo_url: `${HOST}${path}`, content: { resource_type: type, language: SW } });
const BG = { english_name: "Bulgarian", ietf_code: "bg", national_name: "български език", direction: "ltr" };

beforeEach(async () => {
  fx = createFixtures();
  flaky = makeFlakyGit(join(fx.root, "flaky"));
  fx.makeRepo("WA-Catalog", "sw_ulb", { "01-GEN.usfm": "\\id GEN\n\\h Mwanzo\n\\c 1\n" });
  fx.makeRepo("WA-Catalog", "sw_tn", { "gen/01.md": "x" });
  api = await startMockDataApi({
    primary: envelope([row("WA-Catalog/sw_ulb", "ulb"), row("WA-Catalog/sw_tn", "tn")]),
    nonPrimary: envelope([{ repo_url: `${HOST}WA-Catalog/bg_ulb`, content: { resource_type: "ulb", language: BG } }]),
  });
  cache = mkdtempSync(join(process.env["TMPDIR"] ?? tmpdir(), "doc-e2e-"));
  process.env["GIT_CONFIG_GLOBAL"] = fx.gitConfig;
  process.env["GIT_CONFIG_NOSYSTEM"] = "1";
});
afterEach(async () => {
  await api.close();
  execFileSync("rm", ["-rf", fx.root, cache]);
});

async function app(env: Record<string, string> = {}) {
  const config = loadConfig({
    DATA_API_URL: api.url, RESOURCE_ASSETS_DIR: cache, ALLOW_MODE_HEADER: "true", GIT_BIN: flaky.bin,
    DATA_API_TIMEOUT_MS: "400", DATA_API_MAX_ATTEMPTS: "3", DATA_API_BACKOFF_BASE_MS: "5", DATA_API_BACKOFF_CAP_MS: "20", DATA_API_DEADLINE_MS: "5000",
    GIT_MAX_ATTEMPTS: "3", GIT_BACKOFF_BASE_MS: "5", GIT_BACKOFF_CAP_MS: "20", ...env,
  });
  return (await buildApp(config, silentLogger())).app;
}
const call = (a: Awaited<ReturnType<typeof app>>, path: string, mode: "as_built" | "fixed") => a.request(path, { headers: { "x-doc-mode": mode } });

describe("data API faults, end to end", () => {
  it("fixed: API down gives 503 with a typed body and Retry-After; the next request after recovery is 200 (nothing cached)", async () => {
    const a = await app();
    api.setMode({ kind: "down" });
    const res = await call(a, "/book_codes_for_lang/sw", "fixed");
    expect(res.status).toBe(503);
    expect(res.headers.get("retry-after")).toBe("5");
    const body = (await res.json()) as { kind: string; transient: boolean; detail: { attempts: number; cause: string } };
    expect(body).toMatchObject({ kind: "catalog_unavailable", transient: true, detail: { attempts: 3, cause: "network" } });
    api.setMode({ kind: "ok" });
    const ok = await call(a, "/book_codes_for_lang/sw", "fixed");
    expect(ok.status).toBe(200);
    expect(((await ok.json()) as unknown[]).length).toBeGreaterThan(0);
  });

  it("as built: API down gives 200 [] (silent), and the empty answer sticks for the TTL even after the API recovers", async () => {
    const a = await app();
    api.setMode({ kind: "http500" });
    const first = await call(a, "/book_codes_for_lang/sw", "as_built");
    expect([first.status, await first.json()]).toEqual([200, []]);
    api.setMode({ kind: "ok" });
    const second = await call(a, "/book_codes_for_lang/sw", "as_built");
    expect(await second.json()).toEqual([]);
  });

  it("fixed: 500 twice then recovery inside one request is invisible to the caller", async () => {
    const a = await app();
    api.setMode({ kind: "flaky", failFirst: 2 });
    const res = await call(a, "/book_codes_for_lang/sw", "fixed");
    expect(res.status).toBe(200);
    expect(res.headers.get("x-doc-stale")).toBeNull();
  });

  it("fixed: a refresh failure after TTL expiry serves the last good catalog (marked stale); recovery clears the mark", async () => {
    const a = await app({ DATA_API_CACHE_TTL_SECONDS: "1" });
    expect((await call(a, "/language_codes_and_names", "fixed")).status).toBe(200);
    await new Promise((r) => setTimeout(r, 1_100));
    api.setMode({ kind: "down" });
    const stale = await call(a, "/language_codes_and_names", "fixed");
    expect(stale.status).toBe(200);
    expect(stale.headers.get("x-doc-stale")).toBe("1");
    api.setMode({ kind: "ok" });
    const fresh = await call(a, "/language_codes_and_names", "fixed");
    expect(fresh.headers.get("x-doc-stale")).toBeNull();
  });

  it.each([["malformed"], ["wrongShape"], ["graphqlError"]] as const)("malformed answer (%s): fixed 503, as built []", async (kind) => {
    const a = await app({ DATA_API_MAX_ATTEMPTS: "2" });
    api.setMode({ kind });
    expect((await call(a, "/book_codes_for_lang/sw", "fixed")).status).toBe(503);
    const b = await app({ DATA_API_MAX_ATTEMPTS: "2" });
    expect(await (await call(b, "/book_codes_for_lang/sw", "as_built")).json()).toEqual([]);
  });

  it("fixed: a row with a null english_name is skipped and reported; as built it empties the whole catalog", async () => {
    const a = await app();
    api.setMode({ kind: "badRow" });
    const fixed = (await (await call(a, "/book_codes_for_lang/sw?findings=1", "fixed")).json()) as { findings: { code: string }[] };
    expect(fixed.findings.map((f) => f.code)).toContain("CatalogRowSkipped");
    const b = await app();
    expect(await (await call(b, "/language_codes_and_names", "as_built")).json()).toEqual([]);
  });

  it("slow data API: fixed answers 503 after bounded attempts, quickly enough, and recovers when it speeds up", async () => {
    const a = await app({ DATA_API_TIMEOUT_MS: "150", DATA_API_MAX_ATTEMPTS: "2" });
    api.setMode({ kind: "slow", delayMs: 2_000 });
    const started = Date.now();
    expect((await call(a, "/language_codes_and_names", "fixed")).status).toBe(503);
    expect(Date.now() - started).toBeLessThan(1_500);
    api.setMode({ kind: "ok" });
    expect((await call(a, "/language_codes_and_names", "fixed")).status).toBe(200);
  });
});

describe("git faults, end to end", () => {
  it("clone fails twice then works: the response is the full answer, no finding", async () => {
    const a = await app();
    flaky.setMode("fail:2");
    const res = await call(a, "/book_codes_for_lang/sw?findings=1", "fixed");
    const body = (await res.json()) as { result: [string, string][]; failedRepos: unknown[] };
    expect(body.failedRepos).toEqual([]);
    expect(body.result.find(([c]) => c === "gen")?.[1]).toBe("Mwanzo");
  });

  it("clone always fails: fixed answers 200 with the repositories it could not get, never silently; the next request heals", async () => {
    const a = await app({ GIT_MAX_ATTEMPTS: "2" });
    flaky.setMode("fail");
    const bad = (await (await call(a, "/book_codes_for_lang/sw?findings=1", "fixed")).json()) as { result: unknown[]; failedRepos: { reason: string }[]; findings: { code: string }[] };
    expect(bad.failedRepos.length).toBe(2);
    expect(bad.findings.filter((f) => f.code === "RepoNotFetched")).toHaveLength(2);
    flaky.setMode("ok");
    const good = (await (await call(a, "/book_codes_for_lang/sw?findings=1", "fixed")).json()) as { result: unknown[]; failedRepos: unknown[] };
    expect(good.failedRepos).toEqual([]);
    expect(good.result.length).toBeGreaterThan(0);
  });

  it("20 concurrent requests for one language clone each repository once and make one catalog request", async () => {
    const a = await app();
    flaky.setMode("slow:250");
    const responses = await Promise.all(Array.from({ length: 20 }, () => call(a, "/book_codes_for_lang/sw", "fixed")));
    expect(responses.every((r) => r.status === 200)).toBe(true);
    expect(new Set(await Promise.all(responses.map((r) => r.text()))).size).toBe(1);
    expect(flaky.cloneCalls()).toBe(2); // sw_ulb and sw_tn; DOC would clone (and delete) them per request
    expect(api.requests().count).toBe(1); // one catalog request for all 20 (single flight); the book endpoints need only the primary query
  });

  it("per-request deadline: the slow clone is reported as not fetched, finishes in the background, and the next request is complete", async () => {
    const a = await app({ REQUEST_DEADLINE_MS: "100" });
    flaky.setMode("slow:700");
    const first = (await (await call(a, "/book_codes_for_lang/sw?findings=1", "fixed")).json()) as { failedRepos: { reason: string }[] };
    expect(first.failedRepos.map((f) => f.reason)).toEqual(["deadline", "deadline"]);
    flaky.setMode("fail"); // from here a fresh clone would fail: success can only come from the clone that was already running
    let second = { failedRepos: [{}] as unknown[], result: [] as unknown[] };
    for (let i = 0; i < 40 && second.failedRepos.length > 0; i += 1) {
      await new Promise((r) => setTimeout(r, 250));
      second = (await (await call(a, "/book_codes_for_lang/sw?findings=1", "fixed")).json()) as typeof second;
    }
    expect(second.failedRepos).toEqual([]);
    expect(second.result.length).toBeGreaterThan(0);
  });
});
