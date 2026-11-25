/** CLI: node tools/mock-data-api.ts <snapshot dir with resp_primary.json and resp_bg.json> <port> */
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { startMockDataApi } from "../test/support/mockDataApi.ts";

const [dir, port] = process.argv.slice(2);
if (!dir || !port) throw new Error("usage: mock-data-api.ts <snapshot dir> <port>");
const api = await startMockDataApi(
  { primary: readFileSync(join(dir, "resp_primary.json"), "utf8"), nonPrimary: readFileSync(join(dir, "resp_bg.json"), "utf8") },
  Number(port),
);
console.log(`mock data API on ${api.url}`);
