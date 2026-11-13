#!/bin/sh
# The Rust arm's check (RFC-0009 §3): `cargo check --locked --message-format=json`, printing
# each compiler message's `rendered` field, which is rustc's own text with its help and
# note lines. Run in the directory holding the candidate, given its file name; the
# candidate is placed at src/main.rs of a package built from Cargo.toml and Cargo.lock, and
# its path is shown as the candidate's file name. Exit 0 no errors (warnings allowed); 1
# errors in the candidate; 2 setup error.
set -u
ARM_HOME=${ARM_HOME:-/opt/arm}
f=$1
command -v jq >/dev/null || {
  echo "run-check.sh: jq not found" >&2
  exit 2
}
[ -d "$ARM_HOME/target" ] || {
  echo "run-check.sh: $ARM_HOME/target is missing (run install.sh)" >&2
  exit 2
}
pkg=$PWD/pkg
mkdir -p "$pkg/src"
cp "$ARM_HOME/Cargo.toml" "$ARM_HOME/Cargo.lock" "$pkg/"
cat -- "$f" >"$pkg/src/main.rs"
msgs=$PWD/cargo.json
export CARGO_TARGET_DIR=$ARM_HOME/target
(cd "$pkg" && cargo check --locked --offline --message-format=json >"$msgs" 2>"$PWD/../cargo.err")
code=$?
display_sed=$(printf '%s' "$f" | sed -e 's/[\\&#]/\\&/g')
rendered=$(jq -r 'select(.reason == "compiler-message") | .message.rendered' "$msgs" |
  sed -e "s#src/main\.rs#${display_sed}#g")
if [ $code -eq 0 ]; then
  [ -n "$rendered" ] && printf '%s\n' "$rendered"
  exit 0
fi
if jq -e 'select(.reason == "compiler-message" and .message.level == "error")' "$msgs" >/dev/null 2>&1 &&
  [ -n "$(jq -r 'select(.reason == "compiler-message" and .message.level == "error") | .message.rendered' "$msgs")" ]; then
  printf '%s\n' "$rendered"
  exit 1
fi
echo "run-check.sh: cargo check failed without a compiler error (setup error):" >&2
cat "$PWD/cargo.err" >&2
exit 2
