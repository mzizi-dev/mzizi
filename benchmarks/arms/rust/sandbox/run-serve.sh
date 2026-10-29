#!/bin/sh
# Serve the Rust arm's candidate on $PORT: build it in a package made from Cargo.toml and
# Cargo.lock (offline, locked), then exec it. The candidate reads PORT itself and listens
# on 0.0.0.0 (the guide says so).
set -eu
ARM_HOME=${ARM_HOME:-/opt/arm}
f=$1
mkdir -p pkg/src
cp "$ARM_HOME/Cargo.toml" "$ARM_HOME/Cargo.lock" pkg/
cat -- "$f" >pkg/src/main.rs
export CARGO_TARGET_DIR=$ARM_HOME/target
(cd pkg && cargo build --locked --offline --quiet >&2)
exec timeout "${ARM_SERVE_TTL:-300}" "$ARM_HOME/target/debug/candidate"
