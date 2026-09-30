#!/bin/sh
# Start B1's Rust reference on $PORT, then exec it so that stopping this process stops it.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
cargo build --release --quiet --manifest-path "$here/Cargo.toml"
exec "$here/target/release/b1-reference"
