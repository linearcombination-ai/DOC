/** Local git fixtures: bare "remote" repositories, a gitconfig that maps a fake host onto them, and a git wrapper that can fail. */
import { execFileSync } from "node:child_process";
import { chmodSync, existsSync, mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";

export const HOST = "https://content.example.test/";

export interface Fixtures {
  root: string;
  remotes: string;
  gitConfig: string;
  env: NodeJS.ProcessEnv;
  /** Create `<owner>/<name>` with the given files (path -> text); returns its URL on the fake host. */
  makeRepo(owner: string, name: string, files: Record<string, string>): string;
  /** Replace the content of a repository with a new commit. */
  update(owner: string, name: string, files: Record<string, string>): void;
}

function git(cwd: string, ...args: string[]): void {
  execFileSync("git", args, { cwd, stdio: "ignore", env: { ...process.env, GIT_CEILING_DIRECTORIES: dirname(cwd), GIT_CONFIG_GLOBAL: "/dev/null", GIT_AUTHOR_NAME: "t", GIT_AUTHOR_EMAIL: "t@t", GIT_COMMITTER_NAME: "t", GIT_COMMITTER_EMAIL: "t@t" } });
}

export function createFixtures(): Fixtures {
  const root = mkdtempSync(join(process.env["TMPDIR"] ?? tmpdir(), "doc-fx-"));
  const remotes = join(root, "remotes");
  mkdirSync(remotes);
  const gitConfig = join(root, "gitconfig");
  writeFileSync(gitConfig, `[url "file://${remotes}/"]\n\tinsteadOf = ${HOST}\n[init]\n\tdefaultBranch = master\n[protocol "file"]\n\tallow = always\n`);
  const work = (owner: string, name: string): string => join(root, "work", owner, name);
  const commit = (owner: string, name: string, files: Record<string, string>): void => {
    const w = work(owner, name);
    mkdirSync(w, { recursive: true });
    if (!existsSync(join(w, ".git"))) git(w, "init", "-q");
    for (const [path, text] of Object.entries(files)) {
      mkdirSync(dirname(join(w, path)), { recursive: true });
      writeFileSync(join(w, path), text);
    }
    git(w, "add", "-A");
    git(w, "commit", "-q", "--allow-empty", "-m", "c");
    mkdirSync(join(remotes, owner), { recursive: true });
    git(root, "clone", "-q", "--bare", w, join(remotes, owner, name));
  };
  return {
    root,
    remotes,
    gitConfig,
    env: { GIT_CONFIG_GLOBAL: gitConfig, GIT_CONFIG_NOSYSTEM: "1" },
    makeRepo(owner, name, files) {
      commit(owner, name, files);
      return `${HOST}${owner}/${name}`;
    },
    update(owner, name, files) {
      execFileSync("rm", ["-rf", join(remotes, owner, name)]);
      commit(owner, name, files);
    },
  };
}

/**
 * A `git` wrapper for fault injection. It appends every invocation to `<dir>/calls.log` and, for
 * `clone`, behaves as the mode file says (`<dir>/mode`, re-read on every call):
 *   ok | fail | fail:<n> (fail the first n clones, then ok) | hang | partial | slow:<ms>
 * Everything else (rev-parse) goes to the real git.
 */
export function makeFlakyGit(dir: string): { bin: string; setMode(mode: string): void; calls(): string[]; cloneCalls(): number } {
  mkdirSync(dir, { recursive: true });
  const bin = join(dir, "git-flaky");
  writeFileSync(join(dir, "mode"), "ok");
  writeFileSync(
    bin,
    `#!/usr/bin/env bash
DIR="${dir}"
echo "$*" >> "$DIR/calls.log"
is_clone=0
for a in "$@"; do [ "$a" = "clone" ] && is_clone=1; done
if [ $is_clone = 0 ]; then exec git "$@"; fi
mode="$(cat "$DIR/mode")"
dest="\${@: -1}"
src="\${@: -2:1}"
# counters are per repository URL, so concurrent clones of different repositories do not race on one file
ck="$DIR/count.$(printf %s "$src" | md5sum | cut -c1-8)"
[ -f "$ck" ] || echo 0 > "$ck"
n=$(( $(cat "$ck") + 1 )); echo $n > "$ck"
case "$mode" in
  ok) exec git "$@" ;;
  fail) echo "fatal: unable to access: Could not resolve host" >&2; exit 128 ;;
  fail:*) k="\${mode#fail:}"; if [ $n -le "$k" ]; then echo "fatal: early EOF" >&2; exit 128; fi; exec git "$@" ;;
  hang) mkdir -p "$dest"; echo hanging > "$dest/partial.txt"; exec sleep 600 ;;
  slow:*) sleep "$(awk "BEGIN{print \${mode#slow:}/1000}")"; exec git "$@" ;;
  partial) mkdir -p "$dest/.git"; echo "ref: refs/heads/master" > "$dest/.git/HEAD"; echo half > "$dest/half.txt"; exit 0 ;;
esac
`,
  );
  chmodSync(bin, 0o755);
  const read = (f: string): string => execFileSync("cat", [join(dir, f)], { encoding: "utf8" });
  return {
    bin,
    setMode: (m) => {
      writeFileSync(join(dir, "mode"), m);
      execFileSync("bash", ["-c", `rm -f ${dir}/count.*`]);
    },
    calls: () => {
      try {
        return read("calls.log").trim().split("\n");
      } catch {
        return [];
      }
    },
    cloneCalls: () => {
      try {
        return read("calls.log").trim().split("\n").filter((l) => l.includes(" clone ") || l.startsWith("clone ") || l.includes("clone --")).length;
      } catch {
        return 0;
      }
    },
  };
}
