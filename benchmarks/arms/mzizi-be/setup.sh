#!/bin/sh
# Warm the cargo cache the lowered packages build from, once, before any episode: a run's
# sandbox has no network (RFC-0009 §4.5), and serve.sh builds with --offline. Building the
# example service fetches exactly the pinned axum and tokio every lowered service uses.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../../.." && pwd)
out="$repo/target/mz-build/warm"
cargo run --quiet --manifest-path "$repo/compiler/Cargo.toml" --bin mz -- \
  build "$repo/examples/registry.mz" --out "$out"
cargo build --release --quiet --manifest-path "$out/Cargo.toml"
echo "mzizi-be: cargo cache warm for axum and tokio at the pins in compiler/src/lower.rs"
