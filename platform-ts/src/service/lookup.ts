/**
 * The orchestration of endpoints 2-7: catalog (adapter) -> plan (core) -> acquire (adapter) ->
 * list and read (shell I/O) -> finish (core). No domain rule lives here: which repositories, which
 * files, which names, in which order, and what a failure means are all decided by the core.
 */
import type { Acquirer, AcquireFailure } from "../acquirer/acquirer.ts";
import { listRepos, readTexts } from "../acquirer/listing.ts";
import type { CatalogClient, CatalogKind } from "../catalog/client.ts";
import type { Config } from "../config.ts";
import type { Core } from "../core/wasm.ts";
import type { Finding, Flags, Located, LookupDto, Repo } from "../core/seam.ts";
import { errorMessage } from "../util/errors.ts";
import type { Log } from "../util/log.ts";

export type Mode = "as_built" | "fixed";

export interface Outcome<T> {
  body: T;
  findings: Finding[];
  /** The catalog was served from the last good answer because a refresh failed. */
  stale: boolean;
  /** Repositories the request needed and could not get (also reported as findings). */
  failedRepos: AcquireFailure[];
}

interface Snapshot {
  id: number;
  repos: Repo[];
  byLang: Map<string, Repo[]>;
  findings: Finding[];
  validationFailure: string | null;
  /** DOC appends its own six entries to the cached catalog in place (`augment_leak`). */
  augmentedLeak: boolean;
}

/** A snapshot plus whether *this* answer came from a stale catalog (the snapshot itself is cached by rows identity). */
interface View {
  snap: Snapshot;
  stale: boolean;
}

export class Lookup {
  readonly #core: Core;
  readonly #catalog: Record<Mode, CatalogClient>;
  readonly #acquirer: Acquirer;
  readonly #config: Config;
  readonly #log: Log;
  readonly #presets: ReturnType<Core["flagPresets"]>;
  /** Normalised catalogs keyed by the rows array they came from: a TTL refresh makes a new array. */
  readonly #snapshots = new Map<Mode, WeakMap<readonly unknown[], Map<string, Snapshot>>>();
  readonly #derivedLangList = new WeakMap<Snapshot, Map<string, [string, string, boolean][]>>();
  #nextSnapshotId = 1;

  constructor(deps: { core: Core; catalog: Record<Mode, CatalogClient>; acquirer: Acquirer; config: Config; log: Log }) {
    this.#core = deps.core;
    this.#catalog = deps.catalog;
    this.#acquirer = deps.acquirer;
    this.#config = deps.config;
    this.#log = deps.log.child({ component: "lookup" });
    this.#presets = deps.core.flagPresets();
    this.#snapshots.set("as_built", new WeakMap());
    this.#snapshots.set("fixed", new WeakMap());
  }

  flagsFor(mode: Mode): Flags {
    return { ...this.#presets[mode], ...this.#config.flagOverrides };
  }

  chapters(): Record<string, number[]> {
    return this.#core.chaptersInBooks();
  }

  async #snapshot(mode: Mode, kind: CatalogKind): Promise<View> {
    const got = await this.#catalog[mode].rows(kind);
    const flags = this.flagsFor(mode);
    const perMode = this.#snapshots.get(mode);
    let byFlag = perMode?.get(got.rows);
    if (!byFlag) {
      byFlag = new Map();
      perMode?.set(got.rows, byFlag);
    }
    const key = `${kind}:${flags.skip_bad_catalog_rows}`;
    let snap = byFlag.get(key);
    if (!snap) {
      const started = performance.now();
      const n = await this.#core.normalizeCatalog(got.rows, kind, flags);
      const byLang = new Map<string, Repo[]>();
      for (const repo of n.repos) {
        const list = byLang.get(repo.language.ietf_code);
        if (list) list.push(repo);
        else byLang.set(repo.language.ietf_code, [repo]);
      }
      if (n.validation_failure) this.#log.warn({ kind, why: n.validation_failure }, "as built, one invalid row empties the whole catalog (and DOC caches that)");
      this.#log.debug({ kind, rows: got.rows.length, kept: n.repos.length, ms: Number((performance.now() - started).toFixed(1)) }, "catalog normalised");
      snap = { id: this.#nextSnapshotId++, repos: n.repos, byLang, findings: n.findings, validationFailure: n.validation_failure, augmentedLeak: false };
      byFlag.set(key, snap);
    }
    return { snap, stale: got.stale };
  }

  /** Endpoint 3. */
  async languages(mode: Mode): Promise<Outcome<[string, string, boolean][]>> {
    const flags = this.flagsFor(mode);
    const primaryView = await this.#snapshot(mode, "primary");
    const nonPrimaryView = await this.#snapshot(mode, "non_primary");
    const primary = primaryView.snap;
    const nonPrimary = nonPrimaryView.snap;
    const cacheKey = `${nonPrimary.id}:${flags.augment_leak && primary.augmentedLeak}:${flags.tolerate_empty_nonprimary}`;
    let derived = this.#derivedLangList.get(primary);
    if (!derived) {
      derived = new Map();
      this.#derivedLangList.set(primary, derived);
    }
    const findings = [...primary.findings, ...nonPrimary.findings];
    let list = derived.get(cacheKey);
    if (!list) {
      const out = this.#core.langCodesAndNames(primary.repos, nonPrimary.repos, { ...flags, augment_leak: flags.augment_leak && primary.augmentedLeak });
      list = out.result;
      derived.set(cacheKey, list);
      findings.push(...out.findings);
    }
    return { body: list, findings, stale: primaryView.stale || nonPrimaryView.stale, failedRepos: [] };
  }

  /** Endpoints 4 and 5. */
  async bookCodes(lang: string, usfmOnly: boolean, mode: Mode, deadlineAt: number): Promise<Outcome<[string, string][]>> {
    const flags = this.flagsFor(mode);
    const view = await this.#snapshot(mode, "primary");
    const snap = view.snap;
    if (flags.augment_leak) snap.augmentedLeak = true;
    const planned = this.#core.plan(usfmOnly ? "book_codes_usfm_only" : "book_codes", lang, snap.byLang.get(lang) ?? [], flags, this.#config.EN_RG_DIR);
    const acquired = await this.#acquire(lang, planned.repos, deadlineAt);
    const wanted = this.#core.wanted(lang, planned.repos, acquired.listing, flags);
    const texts = await readTexts(wanted, acquired.real);
    const out = this.#core.finishBookCodes({ lang, usfmOnly, repos: planned.repos, listing: acquired.listing, texts, flags });
    if (out.swallowed) this.#log.warn({ lang, swallowed: out.swallowed }, "as built, DOC returns an empty list for this language");
    return { body: out.result ?? [], findings: [...snap.findings, ...out.findings, ...acquired.findings], stale: view.stale, failedRepos: acquired.failed };
  }

  /** Endpoint 6: DOC calls the list path twice; the second language only supplies a set of codes. */
  async sharedBookCodes(lang0: string, lang1: string, mode: Mode, deadlineAt: number): Promise<Outcome<[string, string][]>> {
    const [a, b] = [await this.bookCodes(lang0, false, mode, deadlineAt), await this.bookCodes(lang1, false, mode, deadlineAt)];
    const codes1 = new Set(b.body.map(([code]) => code));
    return {
      body: a.body.filter(([code]) => codes1.has(code)),
      findings: [...a.findings, ...b.findings],
      stale: a.stale || b.stale,
      failedRepos: [...a.failedRepos, ...b.failedRepos],
    };
  }

  /** P5: the repositories of a language as the core locates them (url, directory name, type), for the job's acquisition stage. */
  async located(lang: string, mode: Mode): Promise<{ located: Located[]; repos: Repo[]; stale: boolean }> {
    const flags = this.flagsFor(mode);
    const view = await this.#snapshot(mode, "primary");
    const repos = view.snap.byLang.get(lang) ?? [];
    const planned = this.#core.plan("book_codes", lang, repos, flags, this.#config.EN_RG_DIR);
    return { located: [...planned.repos], repos, stale: view.stale };
  }

  /**
   * The document path's lookup: DOC's `resource_lookup_dto` for every requested resource, over the primary catalog plus the entries DOC adds
   * itself. A resource the catalog has no repository for comes back as `null` (DOC drops those and builds the document from the rest).
   */
  async resolve(requests: readonly { lang_code: string; resource_type: string; book_code: string }[], mode: Mode): Promise<{ dtos: (LookupDto | null)[]; repos: Map<string, Repo>; stale: boolean }> {
    const view = await this.#snapshot(mode, "primary");
    const langs = [...new Set(requests.map((r) => r.lang_code))];
    const rows: Repo[] = [];
    for (const lang of langs) rows.push(...(view.snap.byLang.get(lang) ?? []));
    const out = this.#core.lookupDtos(rows, requests, this.#config.EN_RG_DIR);
    const repos = new Map<string, Repo>();
    for (const repo of rows) repos.set(repo.repo_url, repo);
    return { dtos: out.dtos, repos, stale: view.stale };
  }

  /** Endpoint 7. */
  async resourceTypes(lang: string, books: string, mode: Mode, deadlineAt: number): Promise<Outcome<[string, string][]>> {
    const flags = this.flagsFor(mode);
    const view = await this.#snapshot(mode, "primary");
    const snap = view.snap;
    if (flags.augment_leak) snap.augmentedLeak = true;
    const planned = this.#core.plan("resource_types", lang, snap.byLang.get(lang) ?? [], flags, this.#config.EN_RG_DIR);
    const acquired = await this.#acquire(lang, planned.repos, deadlineAt);
    const out = this.#core.finishResourceTypes({ lang, books, repos: planned.repos, listing: acquired.listing, flags });
    if (out.swallowed) this.#log.warn({ lang, swallowed: out.swallowed }, "as built, DOC returns an empty list for this language");
    return { body: out.result ?? [], findings: [...snap.findings, ...out.findings, ...acquired.findings], stale: view.stale, failedRepos: acquired.failed };
  }

  async #acquire(lang: string, repos: readonly Located[], deadlineAt: number) {
    const started = performance.now();
    const fetchable = repos.filter((r) => r.fetch);
    const result = await this.#acquirer.acquire(fetchable.map((r) => r.url), deadlineAt);
    const dirs = new Map<string, string>();
    const findings: Finding[] = [];
    for (const repo of fetchable) {
      const got = result.acquired.get(repo.url);
      if (!got) continue;
      if (dirs.has(repo.dir)) findings.push({ code: "DirNameCollision", severity: "warn", lang, repo: repo.dir, detail: `two repositories map to the directory name ${repo.dir}; the later one is used (as built, the later clone replaces the earlier)` });
      dirs.set(repo.dir, got.path);
      if (got.provenance === "stale_served") findings.push({ code: "RepoServedStale", severity: "warn", lang, repo: repo.dir, detail: got.refreshError ?? "refresh failed" });
    }
    for (const f of result.failed) {
      const dir = repos.find((r) => r.url === f.url)?.dir;
      findings.push({ code: "RepoNotFetched", severity: "error", lang, ...(dir ? { repo: dir } : {}), detail: `${f.url}: ${f.reason}: ${f.detail} (after ${f.attempts} attempts)` });
    }
    const { listing, real } = await listRepos(dirs).catch((error: unknown) => {
      throw new Error(`listing failed: ${errorMessage(error)}`);
    });
    this.#log.debug({ lang, repos: fetchable.length, failed: result.failed.length, ms: Math.round(performance.now() - started) }, "repositories acquired and listed");
    return { listing, real, findings, failed: result.failed };
  }
}
