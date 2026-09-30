#!/bin/sh
# Serve one Mzizi backend candidate on $PORT, for probing (RFC-0009 §2.3):
#
#   mzprobe serve <task-dir> -- benchmarks/arms/mzizi-be/serve.sh <candidate.mz>
#
# It lowers the candidate with `mz build`, builds the package, and execs the server, so that
# stopping this process stops it. A candidate that passed `mz check` but does not build or
# start is `start_failed`, and fails every fact: RFC-0009 §2.3 counts that on purpose.
set -eu
candidate=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../../.." && pwd)
out="${MZ_BUILD_DIR:-$(dirname "$candidate")/build}"
cargo run --quiet --manifest-path "$repo/compiler/Cargo.toml" --bin mz -- \
  build "$candidate" --out "$out" >&2
cargo build --release --quiet --offline --manifest-path "$out/Cargo.toml"
bin=$(sed -n 's/^name = "\(.*\)"$/\1/p' "$out/Cargo.toml" | head -n 1)
exec "$out/target/release/$bin"
