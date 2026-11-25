#!/usr/bin/env bash
# A stand-in for `git clone ... URL DEST` (what the acquirer runs): copies fixtures/content/<last URL segment>.
# FAKEGIT_CONTENT names the content root. Used by the benchmark; the tests carry their own copy with fault modes.
url="${@: -2:1}"; dest="${@: -1}"; base=$(basename "$url")
mkdir -p "$dest/.git/objects"; cp -r "$FAKEGIT_CONTENT/$base/." "$dest/"
echo "[core]" > "$dest/.git/config"; echo "ref: refs/heads/master" > "$dest/.git/HEAD"
