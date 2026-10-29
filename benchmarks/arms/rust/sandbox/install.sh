#!/bin/sh
# Fetch Cargo.lock's exact dependency tree into CARGO_HOME and compile it once, in this
# directory's target/ (the image's /opt/arm, or the arm's sandbox/ for the local runtime), so
# a check (`cargo check`) and a build (`cargo build`) later need no network and recompile
# only the candidate. `--locked` fails rather than change the lockfile.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
cd "$here"
export CARGO_TARGET_DIR=$here/target
cargo fetch --locked
cargo check --locked --offline
cargo build --locked --offline
rustc --version
cargo --version
