#!/usr/bin/env bash
# Build the Rust core for wasm and generate the Node bindings.
#   core/pkg       the full module (worker: parse and assemble the document) .... default features
#   core/pkg-api   the light module (API: request keys, catalog and book lists) .. --no-default-features (no interleaver, markdown, USFM)
#   core/pkg-fault the full module plus a `debug_spin` op, for the test that proves a hung wasm call can be killed .... FAULT=1
# Needs: rustup stable + target wasm32-unknown-unknown, wasm-bindgen CLI 0.2.130 (the version Cargo.lock pins; WASM_BINDGEN=/path to override).
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
cd "$HERE/core"
WBG="${WASM_BINDGEN:-wasm-bindgen}"
TARGET="${CARGO_TARGET_DIR:-target}"
bind() { # <out dir>
  rm -rf "$1"
  "$WBG" --target nodejs --out-dir "$1" "$TARGET/wasm32-unknown-unknown/release/doc_wasm.wasm" 2>&1 | grep -v target_features || true
  # wasm-bindgen --target nodejs emits CommonJS; this package is "type":"module", so the output directory says so itself
  echo '{"type":"commonjs"}' > "$1/package.json"
  ls -l "$1/doc_wasm_bg.wasm"
}
cargo build --release -p doc-wasm --target wasm32-unknown-unknown
bind pkg
cargo build --release -p doc-wasm --target wasm32-unknown-unknown --no-default-features
bind pkg-api
if [ "${FAULT:-0}" = "1" ]; then
  cargo build --release -p doc-wasm --target wasm32-unknown-unknown --features fault-injection
  bind pkg-fault
  cargo build --release -p doc-wasm --target wasm32-unknown-unknown   # leave the target directory holding the production build
fi
