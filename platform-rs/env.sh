# source me: toolchain on the worktree volume, resource guard (P7, Rust platform)
PR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
WT=$(dirname "$PR")
export SCRATCH=$WT/scratch
export CARGO_HOME=$SCRATCH/cargo RUSTUP_HOME=$SCRATCH/rustup CARGO_TARGET_DIR=$SCRATCH/target CARGO_BUILD_JOBS=2
export npm_config_cache=$SCRATCH/npm-cache npm_config_prefix=$SCRATCH/npm-global
export PATH=$SCRATCH/node/bin:$CARGO_HOME/bin:$SCRATCH/npm-global/bin:$SCRATCH/bin:$SCRATCH/servers/bin:$PATH
export TMPDIR=$SCRATCH/tmp
mkdir -p "$TMPDIR" "$SCRATCH/bin"
export PR WT
