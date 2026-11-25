#!/usr/bin/env bash
# Build Redis 8.12.0 and Valkey 9.2.0 from source into .p6env/ (no docker, no package manager). About 9 minutes for
# Redis (the LTO link is most of it) and 6 for Valkey on 2 cores. Run from p6rs/ after `. ./env.sh`.
set -euo pipefail
: "${P6:?source env.sh first}"
REDIS_TAG="${REDIS_TAG:-8.12.0}"; VALKEY_TAG="${VALKEY_TAG:-9.2.0}"
mkdir -p "$P6/.p6env/src"
build() { # name url tag
  local name=$1 url=$2 tag=$3
  [ -x "$P6/.p6env/$name/bin/$name-server" ] && { echo "$name already built"; return; }
  [ -d "$P6/.p6env/src/$name" ] || git clone -q --depth 1 --branch "$tag" "$url" "$P6/.p6env/src/$name"
  ( cd "$P6/.p6env/src/$name" && nice -n 10 make -j2 BUILD_TLS=yes PREFIX="$P6/.p6env/$name" install > "$P6/.p6env/$name-build.log" 2>&1 )
  "$P6/.p6env/$name/bin/$name-server" --version
}
build redis  https://github.com/redis/redis.git            "$REDIS_TAG"
build valkey https://github.com/valkey-io/valkey.git       "$VALKEY_TAG"
