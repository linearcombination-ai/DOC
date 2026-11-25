import { existsSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { parentPort, workerData } from "node:worker_threads";

interface Wasm {
  call(op: string, input: string): string;
}
interface Data {
  pkgDir: string;
  fault: string;
}
interface Request {
  id: number;
  op: string;
  input: string;
}

const data = workerData as Data;
const wasm = createRequire(import.meta.url)(`${data.pkgDir}/doc_wasm.js`) as Wasm;

parentPort?.on("message", (m: Request) => {
  try {
    // Test-only faults on the assemble call: a call that never returns, so the host must kill the thread.
    //   spin                 forever, in JavaScript;  spin_once:<marker>  the first time only (the marker file records it), so the retry succeeds
    //   wasm_spin            forever, inside the wasm module (needs the fault-injection build, core/pkg-fault)
    if (m.op === "job_assemble" && data.fault !== "") {
      const [kind, marker] = data.fault.split(":", 2) as [string, string | undefined];
      const fire = kind === "spin" || kind === "wasm_spin" || (kind === "spin_once" && marker !== undefined && !existsSync(marker));
      if (fire && kind === "spin_once" && marker !== undefined) writeFileSync(marker, "fired");
      if (fire && kind === "wasm_spin") wasm.call("debug_spin", "x");
      if (fire) for (;;) Math.sqrt(2);
    }
    parentPort?.postMessage({ id: m.id, ok: true, output: wasm.call(m.op, m.input) });
  } catch (e) {
    parentPort?.postMessage({ id: m.id, ok: false, error: e instanceof Error ? e.message : String(e) });
  }
});
