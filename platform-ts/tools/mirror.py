"""Build the pinned local mirror of the content repositories the corpus touches.

usage: python mirror.py <snapshot dir> <corpus.json> <mirror dir> [workers]
For every corpus language: every catalog repository of the language (primary query), the bg
non-primary rows, and the six repositories DOC adds itself. Each is cloned `--bare --depth 1`
into <mirror>/<owner>/<repo>. Writes <mirror>/mirror.lock.tsv (url, commit sha or FAILED, reason):
that file is the "commit hash" half of "reproducible by language code and commit hash".
Both DOC and the shell then clone through a git `insteadOf` rewrite to this directory.
"""
import json
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

HOST = "https://content.bibletranslationtools.org/"
snap, corpus_path, mirror = Path(sys.argv[1]), Path(sys.argv[2]), Path(sys.argv[3])
workers = int(sys.argv[4]) if len(sys.argv) > 4 else 4
langs = {c["lang"] for c in json.loads(corpus_path.read_text())}
primary = json.loads((snap / "resp_primary.json").read_text())["data"]["git_repo"]
bg = json.loads((snap / "resp_bg.json").read_text())["data"]["git_repo"]
urls = {r["repo_url"] for r in primary + bg if r["content"] and r["content"]["language"] and r["content"]["language"]["ietf_code"] in langs}
urls |= {HOST + p for p in (
    "WA-Catalog/bg_bpb", "WycliffeAssociates/en_tn_condensed", "WA-Catalog/id_tbi", "WA-Catalog/id_ayt", "WA-Catalog/id_tq", "WA-Catalog/id_tw")}
lock = mirror / "mirror.lock.tsv"
done = {}
if lock.exists():
    for line in lock.read_text().splitlines():
        u, sha, why = (line.split("\t") + ["", ""])[:3]
        done[u] = (sha, why)


def clone(url: str) -> tuple[str, str, str]:
    if url in done and done[url][0] != "FAILED":
        return url, done[url][0], ""
    dest = mirror / url.removeprefix(HOST)
    if dest.exists():
        subprocess.run(["rm", "-rf", str(dest)], check=True)
    dest.parent.mkdir(parents=True, exist_ok=True)
    r = subprocess.run(["git", "clone", "-q", "--bare", "--depth", "1", url, str(dest)], capture_output=True, text=True, timeout=600)
    if r.returncode != 0:
        return url, "FAILED", r.stderr.strip().replace("\n", " ")[:200]
    sha = subprocess.run(["git", "-C", str(dest), "rev-parse", "HEAD"], capture_output=True, text=True).stdout.strip()
    return url, sha, ""


mirror.mkdir(parents=True, exist_ok=True)
with ThreadPoolExecutor(workers) as pool:
    results = list(pool.map(clone, sorted(urls)))
lock.write_text("".join(f"{u}\t{sha}\t{why}\n" for u, sha, why in results))
failed = [r for r in results if r[1] == "FAILED"]
print(f"{len(results)} repositories, {len(failed)} failed")
