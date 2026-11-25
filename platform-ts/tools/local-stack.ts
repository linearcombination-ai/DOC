/**
 * The local data stack for the README path (no network needed): a mock of DOC's catalog API and local git "remotes" holding a
 * real slice of the English content (Jude: ULB, TN, TQ, TW from test/fixtures/content), under a fake host that git rewrites to the
 * local directories (GIT_CONFIG_GLOBAL). Writes <dir>/stack.env with the variables the service needs, then serves until stopped.
 *
 *   node tools/local-stack.ts <dir> [mock port=18390]
 */
import { mkdirSync, readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { startMockDataApi } from "../test/support/mockDataApi.ts";
import { createFixtures, HOST } from "../test/support/gitFixtures.ts";
import { envelope } from "../test/support/snapshotFixture.ts";

const dir = resolve(process.argv[2] ?? "run");
const port = Number(process.argv[3] ?? 18390);
const content = resolve(import.meta.dirname, "../test/fixtures/content");
mkdirSync(dir, { recursive: true });

function tree(root: string, rel = ""): Record<string, string> {
  const out: Record<string, string> = {};
  for (const name of readdirSync(join(root, rel))) {
    const p = rel ? `${rel}/${name}` : name;
    if (statSync(join(root, p)).isDirectory()) Object.assign(out, tree(root, p));
    else out[p] = readFileSync(join(root, p), "utf8");
  }
  return out;
}

const fx = createFixtures();
const EN = { english_name: "English", ietf_code: "en", national_name: "English", direction: "ltr" };
const rows: unknown[] = [];
for (const [name, type] of [["en_ulb", "ulb"], ["en_tn", "tn"], ["en_tq", "tq"], ["en_tw", "tw"]] as const) {
  fx.makeRepo("WycliffeAssociates", name, tree(join(content, name)));
  rows.push({ repo_url: `${HOST}WycliffeAssociates/${name}`, content: { resource_type: type, language: EN } });
}
const api = await startMockDataApi({ primary: envelope(rows), nonPrimary: envelope([]) }, port);
writeFileSync(join(dir, "stack.env"), [`DATA_API_URL=${api.url}`, `GIT_CONFIG_GLOBAL=${fx.gitConfig}`, "GIT_CONFIG_NOSYSTEM=1", ""].join("\n"));
console.log(`local stack ready: catalog mock ${api.url}, git remotes under ${fx.remotes}, environment in ${join(dir, "stack.env")}`);
const stop = (): void => void api.close().then(() => process.exit(0));
process.on("SIGTERM", stop);
process.on("SIGINT", stop);
setInterval(() => undefined, 60_000);
