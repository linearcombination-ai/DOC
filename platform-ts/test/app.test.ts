import { execFileSync } from "node:child_process";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { buildApp } from "../src/main.ts";
import { loadConfig, type Config } from "../src/config.ts";
import { silentLogger } from "../src/util/log.ts";
import { BookList, ChaptersInBooks, Health, LanguageList, ResourceTypeList, withFindings } from "../src/contract/schemas.ts";
import { createFixtures, HOST, type Fixtures } from "./support/gitFixtures.ts";
import { startMockDataApi, type MockDataApi } from "./support/mockDataApi.ts";
import { envelope } from "./support/snapshotFixture.ts";

let fx: Fixtures;
let api: MockDataApi;
let built: Awaited<ReturnType<typeof buildApp>>;
let noBg: Awaited<ReturnType<typeof buildApp>>;
let apiNoBg: MockDataApi;
let cache: string;
const get = (path: string, mode?: "as_built" | "fixed", init: RequestInit = {}) =>
  built.app.request(path, { ...init, headers: { ...(init.headers ?? {}), ...(mode ? { "x-doc-mode": mode } : {}) } });

const L = (english: string, code: string, national: string) => ({ english_name: english, ietf_code: code, national_name: national, direction: "ltr" });
const row = (path: string, type: string, lang: ReturnType<typeof L>) => ({ repo_url: `${HOST}${path}`, content: { resource_type: type, language: lang } });
const SW = L("Swahili", "sw", "Kiswahili");
const LN = L("Lingala", "ln", "Lingála");
const RW = L("Kinyarwanda", "rw", "Ikinyarwanda");
const ES = L("Spanish", "es-419", "Español");

beforeAll(async () => {
  fx = createFixtures();
  fx.makeRepo("WA-Catalog", "sw_ulb", {
    "01-GEN.usfm": "\\id GEN\n\\h MWANZO\n\\c 1\n\\v 1 x\n",
    "41-MAT.usfm": "\\id MAT\n\\toc2 Mathayo\n\\c 1\n",
    "manifest.yaml": "projects:\n- identifier: gen\n  title: Mwanzo (manifest)\n- identifier: mrk\n  title: Marko\n",
  });
  fx.makeRepo("WA-Catalog", "sw_tn", { "gen/01/01.md": "x", "rev/01/01.md": "x", "README.md": "x" });
  fx.makeRepo("WA-Catalog", "sw_tw", { "bible/kt/god.md": "x" });
  fx.makeRepo("x", "ln_mat_text_reg", { "front/title.txt": "Matai\u0000\n", "01/01.usx": "x" });
  fx.makeRepo("x", "ln_mrk_text_reg", { "front/title.txt": "MARKO" });
  fx.makeRepo("DCS-Mirror", "faustin-azaza_rw_php_text_reg", { "front/title.txt": "Filipi" });
  fx.makeRepo("DCS-Mirror", "rw_jhn_text_reg", { "front/title.txt": "Yohana" });
  fx.makeRepo("WA-Catalog", "es-419_ulb", { "01-GEN.usfm": "\\id GEN\n\\h Génesis\n\\c 1\n" });
  const rows = [
    row("WA-Catalog/sw_ulb", "ulb", SW),
    row("WA-Catalog/sw_tn", "tn", SW),
    row("WA-Catalog/sw_tw", "tw", SW),
    row("x/ln_mat_text_reg", "reg", LN),
    row("x/ln_mrk_text_reg", "reg", LN),
    row("DCS-Mirror/faustin-azaza_rw_php_text_reg", "reg", RW),
    row("DCS-Mirror/rw_jhn_text_reg", "reg", RW),
    row("WA-Catalog/es-419_ulb", "ulb", ES),
    row("WA-Catalog/es-419_tn", "tn", ES), // never created upstream: a repository that cannot be fetched
  ];
  const bg = L("Bulgarian", "bg", "български език");
  api = await startMockDataApi({ primary: envelope(rows), nonPrimary: envelope([row("WA-Catalog/bg_ulb", "ulb", bg)]) });
  apiNoBg = await startMockDataApi({ primary: envelope(rows), nonPrimary: envelope([]) });
  cache = mkdtempSync(join(process.env["TMPDIR"] ?? tmpdir(), "doc-app-"));
  process.env["GIT_CONFIG_GLOBAL"] = fx.gitConfig;
  process.env["GIT_CONFIG_NOSYSTEM"] = "1";
  const config: Config = loadConfig({ DATA_API_URL: api.url, RESOURCE_ASSETS_DIR: cache, ALLOW_MODE_HEADER: "true", GIT_MAX_ATTEMPTS: "1", GIT_BACKOFF_BASE_MS: "1", DATA_API_MAX_ATTEMPTS: "2", DATA_API_BACKOFF_BASE_MS: "1", DATA_API_TIMEOUT_MS: "1500" });
  built = await buildApp(config, silentLogger());
  noBg = await buildApp({ ...config, DATA_API_URL: apiNoBg.url }, silentLogger());
});
afterAll(async () => {
  await api.close();
  await apiNoBg.close();
  execFileSync("rm", ["-rf", fx.root, cache]);
});

describe("endpoints 1-2 (static)", () => {
  it("health: the tuple FastAPI serialises as a two-element array", async () => {
    const res = await get("/health/status");
    expect(res.status).toBe(200);
    expect(await res.text()).toBe('[{"status":"ok"},200]');
  });
  it("chapters_in_books: 66 books in DOC's key order, chapters 1..N", async () => {
    const body = (await (await get("/chapters_in_books")).json()) as Record<string, number[]>;
    expect(Object.keys(body)).toHaveLength(66);
    expect(Object.keys(body).slice(0, 3)).toEqual(["gen", "exo", "lev"]);
    expect(Object.keys(body).at(-1)).toBe("rev");
    expect(body["gen"]).toEqual(Array.from({ length: 50 }, (_, i) => i + 1));
  });
});

describe("book lists", () => {
  it("whole-language USFM repo: header marker order, manifest fallback, DOC's normaliser, English fills the rest", async () => {
    const body = (await (await get("/book_codes_for_lang/sw", "as_built")).json()) as [string, string][];
    const names = Object.fromEntries(body);
    expect(names["gen"]).toBe("Mwanzo"); // \h MWANZO -> normaliser lower-cases after the first letter
    expect(names["mat"]).toBe("Mathayo");
    expect(names["rev"]).toBe("Revelation"); // from the tn directory name, English (as built)
    expect(body[0]?.[0]).toBe("gen");
  });
  it("usfm_only: English names only where USFM gives none", async () => {
    const body = (await (await get("/book_codes_for_lang_from_usfm_only/sw", "as_built")).json()) as [string, string][];
    expect(Object.fromEntries(body)["rev"]).toBe("Revelation");
    expect(body.find(([c]) => c === "gen")?.[1]).toBe("Mwanzo");
  });
  it("per-book repositories: the whole title file, cleaned, normalised", async () => {
    const body = await (await get("/book_codes_for_lang/ln", "as_built")).json();
    expect(body).toEqual([["mat", "Matai"], ["mrk", "Marko"]]);
  });
  it("AS BUILT the owner-prefixed repository empties the list (the KeyError DOC swallows); FIXED lists both books and says what it did", async () => {
    expect(await (await get("/book_codes_for_lang/rw", "as_built")).json()).toEqual([]);
    const res = await get("/book_codes_for_lang/rw?findings=1", "fixed");
    const body = (await res.json()) as { result: [string, string][]; findings: { code: string }[] };
    expect(body.result).toEqual([["jhn", "Yohana"], ["php", "Filipi"]]);
    expect(res.headers.get("x-doc-mode")).toBe("fixed");
  });
  it("as built, a repository that cannot be fetched (tn) empties the list; fixed lists the rest and names the repository", async () => {
    expect(await (await get("/book_codes_for_lang/es-419", "as_built")).json()).toEqual([]);
    const body = (await (await get("/book_codes_for_lang/es-419?findings=1", "fixed")).json()) as { result: [string, string][]; findings: { code: string; detail: string }[]; failedRepos: { url: string }[] };
    expect(body.result).toEqual([["gen", "Génesis"]]);
    expect(body.failedRepos.map((f) => f.url)).toEqual([`${HOST}WA-Catalog/es-419_tn`]);
    expect(body.findings.some((f) => f.code === "RepoNotFetched")).toBe(true);
  });
  it("unknown language: an empty list, as DOC", async () => {
    expect(await (await get("/book_codes_for_lang/zz-none", "as_built")).json()).toEqual([]);
  });
  it("shared_book_codes: order and names of the first language", async () => {
    const body = await (await get("/shared_book_codes/ln/rw", "fixed")).json();
    expect(body).toEqual([]);
    const sw = (await (await get("/shared_book_codes/sw/ln", "fixed")).json()) as [string, string][];
    expect(sw).toEqual([["mat", "Mathayo"]]); // mrk is only in sw's manifest, which names books but never adds them
  });
});

describe("resource types", () => {
  it("lists the types that have the requested books; tw is listed without being cloned", async () => {
    const body = await (await get("/resource_types/sw/gen,mat", "as_built")).json();
    expect(body).toEqual([["tn", "Translation Notes"], ["tw", "Translation Words"], ["ulb", "Unlocked Literal Bible"]]);
  });
  it("a type whose books are not requested is left out (tn needs gen/rev etc.)", async () => {
    const body = await (await get("/resource_types/sw/mat", "as_built")).json();
    expect(body).toEqual([["tw", "Translation Words"], ["ulb", "Unlocked Literal Bible"]]);
  });
});

describe("language list and the state DOC leaks into it", () => {
  it("as built, DOC's own six catalog entries appear in /language_codes_and_names only after a book request", async () => {
    const fresh = await buildApp({ ...built.config }, silentLogger()); // a new process: cold caches
    const list = async (mode: string) => ((await (await fresh.app.request("/language_codes_and_names", { headers: { "x-doc-mode": mode } })).json()) as [string, string, boolean][]).map((x) => x[0]);
    expect(await list("as_built")).not.toContain("id");
    await fresh.app.request("/book_codes_for_lang/sw", { headers: { "x-doc-mode": "as_built" } });
    expect(await list("as_built")).toContain("id"); // the leak: DOC appended to its cached catalog in place
    expect(await list("fixed")).not.toContain("id"); // fixed mode has its own cache and never mutates it
  });
  it("as built an empty non-primary (bg) catalog empties the list; fixed answers and says so", async () => {
    const asBuilt = await noBg.app.request("/language_codes_and_names", { headers: { "x-doc-mode": "as_built" } });
    expect(await asBuilt.json()).toEqual([]);
    const body = (await (await noBg.app.request("/language_codes_and_names?findings=1", { headers: { "x-doc-mode": "fixed" } })).json()) as { result: unknown[]; findings: { code: string }[] };
    expect(body.result.length).toBeGreaterThan(3);
    expect(body.findings.map((f) => f.code)).toContain("NonPrimaryCatalogEmpty");
  });
});

describe("HTTP behaviour FastAPI has for these paths", () => {
  it("404 and 405 bodies", async () => {
    const nf = await get("/nonexistent");
    expect([nf.status, await nf.json()]).toEqual([404, { detail: "Not Found" }]);
    const na = await get("/health/status", undefined, { method: "POST" });
    expect([na.status, await na.json(), na.headers.get("allow")]).toEqual([405, { detail: "Method Not Allowed" }, "GET"]);
  });
  it("HEAD on a GET route is 405 and an encoded slash does not stay inside a path parameter (both found by the oracle comparison)", async () => {
    const head = await get("/chapters_in_books", undefined, { method: "HEAD" });
    expect([head.status, head.headers.get("allow")]).toEqual([405, "GET"]);
    const slash = await get("/book_codes_for_lang/a%2Fb");
    expect([slash.status, await slash.json()]).toEqual([404, { detail: "Not Found" }]);
  });
  it("trailing slash redirects with 307 to the slashless path", async () => {
    const res = await get("/health/status/");
    expect(res.status).toBe(307);
    expect(new URL(res.headers.get("location") ?? "").pathname).toBe("/health/status");
  });
  it("CORS: allowed origin gets credentials headers, other origins do not", async () => {
    const ok = await get("/health/status", undefined, { headers: { origin: "http://localhost:5173" } });
    expect(ok.headers.get("access-control-allow-origin")).toBe("http://localhost:5173");
    expect(ok.headers.get("access-control-allow-credentials")).toBe("true");
    const other = await get("/health/status", undefined, { headers: { origin: "http://evil.example" } });
    expect(other.headers.get("access-control-allow-origin")).toBeNull();
  });
  it("path parameters: any string as built; fixed mode answers 422 in the legacy body shape", async () => {
    const lenient = await get("/book_codes_for_lang/x%20y", "as_built");
    expect([lenient.status, await lenient.json()]).toEqual([200, []]);
    const strict = await get("/book_codes_for_lang/x%20y", "fixed");
    expect(strict.status).toBe(422);
    const body = (await strict.json()) as { status_code: number; message: string; data: null };
    expect(body.status_code).toBe(10422);
    expect(body.data).toBeNull();
    const books = await get("/resource_types/sw/GEN", "fixed");
    expect(books.status).toBe(422);
  });
});

describe("the OpenAPI document comes from the same route objects", () => {
  it("describes all seven endpoints, the tuple shapes and the legacy 422 body", async () => {
    const doc = (await (await get("/openapi.json")).json()) as { openapi: string; paths: Record<string, unknown>; components: { schemas: Record<string, unknown> } };
    expect(doc.openapi).toBe("3.1.0");
    expect(Object.keys(doc.paths).sort()).toEqual([
      "/book_codes_for_lang/{lang_code}",
      "/book_codes_for_lang_from_usfm_only/{lang_code}",
      "/chapters_in_books",
      "/health/status",
      "/language_codes_and_names",
      "/resource_types/{lang_code}/{book_codes}",
      "/shared_book_codes/{lang0_code}/{lang1_code}",
    ]);
    expect(Object.keys(doc.components.schemas)).toContain("LegacyValidationError");
    expect(JSON.stringify(doc)).toContain("prefixItems");
  });
});

describe("contract conformance: every response parses with the schema the OpenAPI document was built from", () => {
  it.each([
    ["/health/status", Health],
    ["/chapters_in_books", ChaptersInBooks],
    ["/language_codes_and_names", LanguageList],
    ["/book_codes_for_lang/sw", BookList],
    ["/book_codes_for_lang_from_usfm_only/sw", BookList],
    ["/shared_book_codes/sw/ln", BookList],
    ["/resource_types/sw/gen,mat", ResourceTypeList],
  ] as const)("%s", async (path, schema) => {
    const res = await get(path, "as_built");
    expect(res.status).toBe(200);
    expect(res.headers.get("content-type")).toContain("application/json");
    expect(schema.safeParse(await res.clone().json()).success).toBe(true);
  });
  it("the findings envelope parses with withFindings()", async () => {
    const body: unknown = await (await get("/book_codes_for_lang/rw?findings=1", "fixed")).json();
    expect(withFindings(BookList).safeParse(body).success).toBe(true);
  });
});
