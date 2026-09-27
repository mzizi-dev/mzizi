#!/usr/bin/env bash
# Dioxus arm compile check. See README.md in this directory.
#
#   check.sh <candidate.rs>
#
# Places the candidate at sandbox/src/component.rs (mounted by sandbox/src/lib.rs as
# `pub mod component;`), runs `cargo check`, and prints the compiler's diagnostics for
# the candidate to stdout, rendered exactly as rustc renders them (the human format,
# help and note lines included) with no colour and no Cargo progress or timing lines.
#
# Exit codes:
#   0  compiles with no errors (warnings are printed and allowed)
#   1  the candidate has compile errors (printed)
#   2  usage or setup error (message on stderr)
set -euo pipefail

usage() {
  echo "usage: $0 <candidate.rs>" >&2
  exit 2
}

[[ $# -eq 1 ]] || usage
candidate=$1
[[ -f $candidate && -r $candidate ]] || {
  echo "check.sh: not a readable file: $candidate" >&2
  exit 2
}
for tool in cargo jq flock; do
  command -v "$tool" >/dev/null || {
    echo "check.sh: required tool not found: $tool" >&2
    exit 2
  }
done

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
sandbox=$here/sandbox
slot=$sandbox/src/component.rs
display=$(basename -- "$candidate")
# Escaped for use as a sed replacement (the delimiter below is #).
display_sed=$(printf '%s' "$display" | sed -e 's/[\\&#]/\\&/g')

# One check at a time per sandbox: two concurrent runs would otherwise overwrite each
# other's candidate. Callers that want parallelism should use separate copies of this
# directory (each gets its own target/).
exec 9>"$sandbox/target.lock"
flock 9

# Reset the crate to its fixed shape before placing the candidate: nothing a previous
# candidate left in src/ survives, whatever it was.
find "$sandbox/src" -mindepth 1 ! -name lib.rs -exec rm -rf -- {} +
cleanup() { rm -f -- "$slot" "${stderr_log:-}"; }
trap cleanup EXIT

# `cat >`, not `cp -p`: the slot gets a fresh mtime, so Cargo's fingerprint always sees
# the new contents, even when two candidates are the same size.
cat -- "$candidate" >"$slot"

stderr_log=$(mktemp)
set +e
# --locked: the lockfile is part of the pin and must never be rewritten.
# RUSTFLAGS and friends are cleared so the caller's environment cannot change what
# the candidate is checked against (and cannot invalidate the warm cache).
json=$(env -u RUSTFLAGS -u CARGO_ENCODED_RUSTFLAGS -u CARGO_BUILD_RUSTFLAGS \
  CARGO_TARGET_DIR="$sandbox/target" CARGO_TERM_COLOR=never \
  cargo check --locked --quiet --message-format=json \
  --manifest-path "$sandbox/Cargo.toml" 2>"$stderr_log")
set -e

# Only diagnostics for the sandbox crate itself; dependency warnings are not the
# candidate's. Paths are made machine-independent: the slot is shown under the
# candidate's own file name (line numbers are the candidate's, since it is copied
# verbatim), and Cargo registry sources as <cargo-registry>/.
rendered=$(jq -r '
  select(.reason == "compiler-message")
  | select(.package_id | test("dioxus-arm-sandbox"))
  | .message.rendered' <<<"$json" |
  sed -E \
    -e "s#src/component\.rs#${display_sed}#g" \
    -e 's#[^ ]*/registry/src/index\.crates\.io-[0-9a-f]+/#<cargo-registry>/#g')

errors=$(jq -r '
  select(.reason == "compiler-message")
  | select(.package_id | test("dioxus-arm-sandbox"))
  | select(.message.level == "error" or .message.level == "error: internal compiler error")
  | 1' <<<"$json" | wc -l)
success=$(jq -r 'select(.reason == "build-finished") | .success' <<<"$json")

[[ -n $rendered ]] && printf '%s\n' "$rendered"

if [[ $success == true ]]; then
  exit 0
elif [[ $errors -gt 0 ]]; then
  exit 1
else
  # Cargo failed without a diagnostic against the candidate: a setup problem (network,
  # lockfile, toolchain), never the candidate's fault.
  echo "check.sh: cargo failed without compiler diagnostics (setup error):" >&2
  cat -- "$stderr_log" >&2
  exit 2
fi
