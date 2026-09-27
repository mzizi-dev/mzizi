#!/usr/bin/env bash
# Verify every ```mz block in a prompt guide against the real compiler.
#
# A block is expected to pass `mz check --agent` and `mz contract --agent` (both exit 0),
# unless its first line contains "WRONG ON PURPOSE", in which case `mz check --agent` must
# exit 1. Each block is written to <out>/block-<n>.mz and its NDJSON kept beside it.
#
# usage: benchmarks/prompts/verify-guide.sh [guide.md] [out-dir]
set -u

root="$(cd "$(dirname "$0")/../.." && pwd)"
guide="${1:-$root/benchmarks/prompts/mzizi-guide.md}"
out="${2:-$(mktemp -d)}"
mkdir -p "$out"

cargo build -q --manifest-path "$root/compiler/Cargo.toml" --bin mz || exit 2
mz="$root/target/debug/mz"

awk -v out="$out" '
  /^```mz$/ { n++; inblock = 1; file = sprintf("%s/block-%d.mz", out, n); next }
  /^```$/ && inblock { inblock = 0; close(file); next }
  inblock { print > file }
' "$guide"

status=0
for block in "$out"/block-*.mz; do
  [ -e "$block" ] || { echo "no mz blocks found in $guide"; exit 2; }
  name="$(basename "$block")"
  "$mz" check --agent "$block" > "$block.check.ndjson"
  check_rc=$?
  "$mz" contract --agent "$block" > "$block.contract.ndjson"
  contract_rc=$?
  if head -n 1 "$block" | grep -q "WRONG ON PURPOSE"; then
    expect="check=1"
    [ "$check_rc" -eq 1 ] && verdict=ok || { verdict=FAIL; status=1; }
  else
    expect="check=0 contract=0"
    [ "$check_rc" -eq 0 ] && [ "$contract_rc" -eq 0 ] && verdict=ok || { verdict=FAIL; status=1; }
  fi
  echo "$verdict  $name  check=$check_rc contract=$contract_rc  (expected $expect)"
done

echo "blocks and NDJSON in $out"
exit "$status"
