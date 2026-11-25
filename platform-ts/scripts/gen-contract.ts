/**
 * Generate the OpenAPI document and the frontend types from the route objects (the contract).
 *   node scripts/gen-contract.ts
 * writes generated/openapi.json, then runs openapi-typescript (installed under tools/gen because it needs
 * TypeScript 5 and the shell is on 7) to write generated/api.d.ts.
 */
import { execFileSync } from "node:child_process";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { buildApp } from "../src/main.ts";
import { loadConfig } from "../src/config.ts";
import { loadJobConfig } from "../src/jobs/config.ts";
import { silentLogger } from "../src/util/log.ts";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const config = loadConfig({ DATA_API_URL: "http://127.0.0.1:9/unused", RESOURCE_ASSETS_DIR: mkdtempSync(join(process.env["TMPDIR"] ?? tmpdir(), "gen-")) });
// the job routes exist only when a queue is configured: give it a Redis URL that nothing will ever connect to
const built = await buildApp(config, silentLogger(), undefined, loadJobConfig({ REDIS_URL: "redis://127.0.0.1:1/0" }), "api");
const app = built.app;
const doc: unknown = await (await app.request("/openapi.json")).json();
writeFileSync(join(root, "generated/openapi.json"), JSON.stringify(doc, null, 2) + "\n");
execFileSync(join(root, "tools/gen/node_modules/.bin/openapi-typescript"), [join(root, "generated/openapi.json"), "-o", join(root, "generated/api.d.ts")], { stdio: "inherit" });

await built.jobs?.service.close();
await built.thread.close();
process.exit(0);
