/**
 * Per-attempt scratch directories and atomic placement of results.
 *
 * DOC's tasks share `working_temp/*` (the hazard in the Celery assessment); here every attempt of every job gets its own
 * directory named with the owner's pid, and a worker that starts removes the directories of owners that no longer exist
 * (a SIGKILLed worker leaves one behind). The finished file is copied or renamed into the output directory under a
 * temporary name and renamed into place, so a reader never sees half a file.
 */
import { randomBytes } from "node:crypto";
import { copyFile, mkdir, open, readdir, rename, rm, stat } from "node:fs/promises";
import { dirname, join } from "node:path";

export async function createScratch(workingDir: string, jobId: string, attempt: number): Promise<string> {
  const dir = join(workingDir, "jobs", `${process.pid}-${safe(jobId)}-${attempt}-${randomBytes(3).toString("hex")}`);
  await mkdir(dir, { recursive: true });
  return dir;
}

export async function removeScratch(dir: string): Promise<void> {
  await rm(dir, { recursive: true, force: true });
}

function safe(id: string): string {
  return id.replace(/[^A-Za-z0-9_-]/g, "_").slice(0, 60);
}

export function ownerAlive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    return (error as NodeJS.ErrnoException).code === "EPERM";
  }
}

/** Remove scratch directories whose owner process is gone. Returns the names removed. */
export async function recoverScratch(workingDir: string): Promise<string[]> {
  const root = join(workingDir, "jobs");
  const removed: string[] = [];
  let names: string[] = [];
  try {
    names = await readdir(root);
  } catch {
    return removed;
  }
  for (const name of names) {
    const pid = Number.parseInt(name.split("-")[0] ?? "", 10);
    if (Number.isInteger(pid) && pid !== process.pid && !ownerAlive(pid)) {
      await rm(join(root, name), { recursive: true, force: true });
      removed.push(name);
    }
  }
  return removed;
}

export async function isFresh(path: string, ttlMs: number, now = Date.now()): Promise<boolean> {
  try {
    const info = await stat(path);
    return info.isFile() && info.size > 0 && now - info.mtimeMs <= ttlMs;
  } catch {
    return false;
  }
}

/** Put `src` at `dest` atomically: rename when on one filesystem, otherwise copy to a temporary name beside `dest` and rename. */
export async function placeAtomically(src: string, dest: string): Promise<void> {
  await mkdir(dirname(dest), { recursive: true });
  try {
    await rename(src, dest);
    return;
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== "EXDEV") throw error;
  }
  const tmp = `${dest}.${randomBytes(4).toString("hex")}.partial`;
  await copyFile(src, tmp);
  const handle = await open(tmp, "r+");
  try {
    await handle.sync();
  } finally {
    await handle.close();
  }
  await rename(tmp, dest);
  await rm(src, { force: true });
}
