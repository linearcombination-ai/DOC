import { readdir, readFile } from "node:fs/promises";
import { join } from "node:path";
import type { Listing, Texts } from "../core/seam.ts";

/** How deep the core's glob and scandir calls reach under a repository directory. */
export const LIST_DEPTH = 2;

async function walk(real: string, rel: string, depth: number, out: { p: string; d: boolean }[]): Promise<void> {
  const entries = await readdir(real, { withFileTypes: true });
  entries.sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0));
  for (const e of entries) {
    if (e.name === ".git" || e.name === ".doc-meta.json") continue;
    const p = `${rel}/${e.name}`;
    const isDir = e.isDirectory();
    out.push({ p, d: isDir });
    if (isDir && depth < LIST_DEPTH) await walk(join(real, e.name), p, depth + 1, out);
  }
}

/** The listing the core sees: `dir` is the name DOC would give the directory, `real` where it is. */
export async function listRepos(repos: ReadonlyMap<string, string>): Promise<{ listing: Listing; real: Map<string, string> }> {
  const listing: Listing = { repos: {} };
  for (const [dir, real] of repos) {
    const entries: { p: string; d: boolean }[] = [];
    try {
      await walk(real, dir, 1, entries);
      listing.repos[dir] = { present: true, entries };
    } catch {
      listing.repos[dir] = { present: false, entries: [] };
    }
  }
  return { listing, real: new Map(repos) };
}

const decoder = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true });

/** What Python's `open(path, "r").read()` returns: UTF-8 (a BOM is kept), `\r\n` and `\r` become `\n`;
 *  `null` when the bytes are not UTF-8 (Python raises UnicodeDecodeError). */
export function pythonText(bytes: Uint8Array): string | null {
  try {
    return decoder.decode(bytes).replace(/\r\n?/g, "\n");
  } catch {
    return null;
  }
}

/** Read the files the core asked for. `wanted` paths start with the DOC directory name. */
export async function readTexts(wanted: readonly string[], real: ReadonlyMap<string, string>): Promise<Texts> {
  const texts: Texts = {};
  await Promise.all(
    wanted.map(async (path) => {
      const slash = path.indexOf("/");
      const root = real.get(path.slice(0, slash));
      if (root === undefined) return;
      try {
        texts[path] = pythonText(await readFile(join(root, path.slice(slash + 1))));
      } catch {
        // a file that vanished between listing and reading is simply absent
      }
    }),
  );
  return texts;
}
