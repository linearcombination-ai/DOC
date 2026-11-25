#!/usr/bin/env bash
# Fetch scripture-kitchen at the pinned commit into vendor/kitchen (read-only dependency of the Rust core, path crates
# canon, onion, bttwriter, mise). Used by the Dockerfiles and by developers.
#   KITCHEN_SRC=<local clone or url>  where to fetch from (default: the public GitHub repository)
#   KITCHEN_REF=<full sha>            the commit (default: v0.1.24 = 0090c0be...)
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SRC="${KITCHEN_SRC:-https://github.com/WycliffeAssociates/scripture-kitchen}"
REF="${KITCHEN_REF:-0090c0beb19cfbd8e08b963a48ae2caeeb339821}"
DEST="${1:-$HERE/vendor/kitchen}"
if [ -d "$DEST/.git" ] && [ "$(git -C "$DEST" rev-parse HEAD 2>/dev/null)" = "$REF" ]; then echo "kitchen already at $REF"; exit 0; fi
rm -rf "$DEST"; mkdir -p "$DEST"
git -C "$DEST" init -q
git -C "$DEST" remote add origin "$SRC"
git -C "$DEST" fetch -q --depth 1 origin "$REF"
git -C "$DEST" checkout -q FETCH_HEAD
echo "kitchen at $(git -C "$DEST" rev-parse HEAD)"
