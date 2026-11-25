import { spawn } from "node:child_process";
import { errorMessage } from "../util/errors.ts";

export interface GitResult {
  ok: boolean;
  code: number | null;
  timedOut: boolean;
  stderr: string;
  ms: number;
}

/**
 * Run git with a hard timeout. git spawns helpers (git-remote-https), so the child leads its own
 * process group and the whole group is killed on timeout; otherwise a hung transfer would outlive
 * the timeout and keep writing into the directory we are about to delete.
 */
export function runGit(gitBin: string, args: readonly string[], options: { timeoutMs: number; cwd?: string; env?: NodeJS.ProcessEnv }): Promise<GitResult> {
  return new Promise<GitResult>((resolve) => {
    const started = performance.now();
    let stderr = "";
    let timedOut = false;
    let settled = false;
    const child = spawn(gitBin, args, {
      ...(options.cwd === undefined ? {} : { cwd: options.cwd }),
      env: { ...process.env, GIT_TERMINAL_PROMPT: "0", ...options.env },
      stdio: ["ignore", "ignore", "pipe"],
      detached: true,
    });
    const finish = (code: number | null, extra = ""): void => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      resolve({ ok: code === 0 && !timedOut, code, timedOut, stderr: (stderr + extra).trim().slice(-2000), ms: Math.round(performance.now() - started) });
    };
    const timer = setTimeout(() => {
      timedOut = true;
      killGroup(child.pid);
    }, options.timeoutMs);
    child.stderr.on("data", (chunk: Buffer) => {
      if (stderr.length < 8192) stderr += chunk.toString("utf8");
    });
    child.on("error", (error) => finish(null, errorMessage(error)));
    child.on("close", (code) => finish(code));
  });
}

function killGroup(pid: number | undefined): void {
  if (pid === undefined) return;
  try {
    process.kill(-pid, "SIGKILL");
  } catch {
    // already gone
  }
}
