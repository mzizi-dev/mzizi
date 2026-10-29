#!/bin/sh
# Start B3's Mzizi reference on $PORT: `mz build` it, build the package, and exec the server
# so that stopping this process stops it (mzprobe verify relies on that).
set -eu
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../../../.." && pwd)
out="$repo/target/mz-build/b3-mirror-reference"
cargo run --quiet --manifest-path "$repo/compiler/Cargo.toml" --bin mz -- \
  build "$here/gateway.mz" --out "$out" >&2
cargo build --release --quiet --manifest-path "$out/Cargo.toml"
exec "$out/target/release/mz-gateway"
