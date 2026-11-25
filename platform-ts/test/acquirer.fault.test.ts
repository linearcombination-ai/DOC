import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { mkdtempSync, readlinkSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { Acquirer, type AcquirerOptions } from "../src/acquirer/acquirer.ts";
import { silentLogger } from "../src/util/log.ts";
import { createFixtures, makeFlakyGit, type Fixtures } from "./support/gitFixtures.ts";

let fx: Fixtures;
let flaky: ReturnType<typeof makeFlakyGit>;
let cacheRoot: string;
let clock: number;
let url: string;

beforeEach(() => {
  fx = createFixtures();
  flaky = makeFlakyGit(join(fx.root, "flaky"));
  cacheRoot = mkdtempSync(join(process.env["TMPDIR"] ?? tmpdir(), "doc-cache-"));
  clock = 5_000_000;
  url = fx.makeRepo("WA-Catalog", "sw_ulb", { "01-GEN.usfm": "\\id GEN\n\\h Mwanzo\n\\c 1\n\\v 1 x\n", "manifest.yaml": "projects: []\n" });
});
afterEach(() => {
  execFileSync("rm", ["-rf", fx.root, cacheRoot]);
});

function acquirer(over: Partial<AcquirerOptions> = {}): Acquirer {
  return new Acquirer({
    root: cacheRoot,
    gitBin: flaky.bin,
    userAgent: "wa-doc",
    requestedWith: "WA-Tool-Doc",
    stalenessMs: 30 * 60_000,
    cloneTimeoutMs: 20_000,
    maxAttempts: 3,
    backoff: { baseMs: 5, capMs: 20 },
    concurrency: 4,
    gcGraceMs: 50,
    log: silentLogger(),
    now: () => clock,
    sleep: () => Promise.resolve(),
    random: () => 0.5,
    env: fx.env,
    ...over,
  });
}
const far = (): number => clock + 600_000;

describe("git clone faults", () => {
  it("fails twice, then succeeds: retried with backoff, attempts reported, repository usable", async () => {
    flaky.setMode("fail:2");
    const r = await acquirer().acquire([url], far());
    expect(r.failed).toEqual([]);
    const got = r.acquired.get(url);
    expect(got?.attempts).toBe(3);
    expect(got?.provenance).toBe("fresh_clone");
    expect(existsSync(join(got?.path ?? "", "01-GEN.usfm"))).toBe(true);
  });

  it("always failing: the result names the repository and the reason; nothing is silently dropped", async () => {
    flaky.setMode("fail");
    const r = await acquirer().acquire([url], far());
    expect(r.acquired.size).toBe(0);
    expect(r.failed).toHaveLength(1);
    expect(r.failed[0]).toMatchObject({ url, reason: "clone_failed", attempts: 3 });
    expect(r.failed[0]?.detail).toContain("128");
  });

  it("no cached failure: after a total failure the next request clones again and succeeds (self-heal)", async () => {
    const a = acquirer();
    flaky.setMode("fail");
    expect((await a.acquire([url], far())).failed).toHaveLength(1);
    flaky.setMode("ok");
    const again = await a.acquire([url], far());
    expect(again.failed).toEqual([]);
    expect(again.acquired.get(url)?.provenance).toBe("fresh_clone");
  });

  it("timeout: the hung git (and its process group) is killed, the temp directory removed, reason is clone_timeout", async () => {
    flaky.setMode("hang");
    const r = await acquirer({ cloneTimeoutMs: 300, maxAttempts: 2 }).acquire([url], far());
    expect(r.failed[0]).toMatchObject({ reason: "clone_timeout", attempts: 2 });
    const left = existsSync(join(cacheRoot, ".tmp")) ? readdirSync(join(cacheRoot, ".tmp")) : [];
    expect(left).toEqual([]);
    const stray = execFileSync("bash", ["-c", "pgrep -f '[s]leep 600' || true"], { encoding: "utf8" }).trim();
    expect(stray).toBe("");
  });

  it("partial clone (git says success, repository is incomplete): detected, retried, never published", async () => {
    flaky.setMode("partial");
    const a = acquirer({ maxAttempts: 2 });
    const r = await a.acquire([url], far());
    expect(r.failed[0]).toMatchObject({ reason: "invalid_repository", attempts: 2 });
    expect(existsSync(join(cacheRoot, Acquirer.keyFor(url), "current"))).toBe(false);
    flaky.setMode("ok");
    expect((await a.acquire([url], far())).acquired.get(url)?.provenance).toBe("fresh_clone");
  });

  it("concurrent requests for the same repository share one clone (in-process single flight, no lock files)", async () => {
    flaky.setMode("slow:300");
    const a = acquirer();
    const results = await Promise.all(Array.from({ length: 12 }, () => a.acquire([url], far())));
    expect(flaky.cloneCalls()).toBe(1);
    expect(new Set(results.map((r) => r.acquired.get(url)?.path)).size).toBe(1);
    expect(a.joined).toBe(11);
    const files = execFileSync("bash", ["-c", `find ${cacheRoot} -name '*.lock' | wc -l`], { encoding: "utf8" }).trim();
    expect(files).toBe("0");
  });

  it("two different repositories clone concurrently and independently", async () => {
    const url2 = fx.makeRepo("WA-Catalog", "sw_tn", { "gen/01.md": "x" });
    flaky.setMode("fail:1");
    const r = await acquirer({ maxAttempts: 2 }).acquire([url, url2], far());
    expect(r.failed).toEqual([]);
    expect(r.acquired.size).toBe(2);
  });
});

describe("cache, staleness and atomic swap", () => {
  it("reuses a repository inside DOC's 30-minute window and refreshes after it", async () => {
    const a = acquirer();
    await a.acquire([url], far());
    clock += 29 * 60_000;
    expect((await a.acquire([url], far())).acquired.get(url)?.provenance).toBe("cache_hit");
    expect(flaky.cloneCalls()).toBe(1);
    clock += 2 * 60_000;
    expect((await a.acquire([url], far())).acquired.get(url)?.provenance).toBe("refreshed");
    expect(flaky.cloneCalls()).toBe(2);
  });

  it("staleness window is a configuration value", async () => {
    const a = acquirer({ stalenessMs: 1_000 });
    await a.acquire([url], far());
    clock += 2_000;
    expect((await a.acquire([url], far())).acquired.get(url)?.provenance).toBe("refreshed");
  });

  it("a failed refresh serves the stale copy and says so; a later refresh succeeds", async () => {
    const a = acquirer();
    const first = await a.acquire([url], far());
    clock += 31 * 60_000;
    flaky.setMode("fail");
    const stale = (await a.acquire([url], far())).acquired.get(url);
    expect(stale?.provenance).toBe("stale_served");
    expect(stale?.refreshError).toContain("clone_failed");
    expect(stale?.path).toBe(first.acquired.get(url)?.path);
    flaky.setMode("ok");
    expect((await a.acquire([url], far())).acquired.get(url)?.provenance).toBe("refreshed");
  });

  it("upstream change is picked up on refresh and the old generation is garbage-collected", async () => {
    const a = acquirer({ gcGraceMs: 20 });
    const first = (await a.acquire([url], far())).acquired.get(url)?.path ?? "";
    fx.update("WA-Catalog", "sw_ulb", { "01-GEN.usfm": "\\id GEN\n\\h Mwanzo mpya\n\\c 1\n" });
    clock += 31 * 60_000;
    const second = (await a.acquire([url], far())).acquired.get(url)?.path ?? "";
    expect(second).not.toBe(first);
    expect(readFileSync(join(second, "01-GEN.usfm"), "utf8")).toContain("mpya");
    await new Promise((r) => setTimeout(r, 120));
    expect(existsSync(first)).toBe(false);
    expect(existsSync(second)).toBe(true);
  });

  it("atomic swap: a reader never sees a missing or half-written repository while refreshes happen", async () => {
    const a = acquirer({ gcGraceMs: 10_000, stalenessMs: 1 });
    await a.acquire([url], far());
    const link = join(cacheRoot, Acquirer.keyFor(url), "current");
    let bad = 0;
    let reads = 0;
    let stop = false;
    const reader = (async () => {
      while (!stop) {
        try {
          const target = readlinkSync(link);
          const real = join(cacheRoot, Acquirer.keyFor(url), target);
          if (!existsSync(join(real, ".git", "HEAD")) || !existsSync(join(real, "01-GEN.usfm"))) bad += 1;
          reads += 1;
        } catch {
          bad += 1;
        }
        await new Promise((r) => setImmediate(r));
      }
    })();
    for (let i = 0; i < 6; i += 1) {
      clock += 10;
      await a.acquire([url], far());
    }
    stop = true;
    await reader;
    expect(reads).toBeGreaterThan(20);
    expect(bad).toBe(0);
  });

  it("per-request deadline: this caller gets a deadline failure, the clone finishes in the background and the next request is a cache hit", async () => {
    flaky.setMode("slow:600");
    const a = acquirer({ now: Date.now }); // a real clock: the deadline is wall time
    const quick = await a.acquire([url], Date.now() + 100);
    expect(quick.failed[0]?.reason).toBe("deadline");
    await new Promise((r) => setTimeout(r, 1_500));
    flaky.setMode("fail"); // would fail if it had to clone again
    const next = await a.acquire([url], far());
    expect(next.failed).toEqual([]);
    expect(next.acquired.get(url)?.provenance).toBe("cache_hit");
  });

  it("recover() removes temp directories left by a crash", async () => {
    const a = acquirer();
    await a.recover();
    execFileSync("mkdir", ["-p", join(cacheRoot, ".tmp", "orphan")]);
    await a.recover();
    expect(readdirSync(join(cacheRoot, ".tmp"))).toEqual([]);
  });
});
