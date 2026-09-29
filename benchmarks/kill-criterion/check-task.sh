#!/usr/bin/env bash
# Check that a task directory is ready to be run: the fixture format is complete, the
# runner loads it, and its reference, scored against itself with the task's own flags,
# checks at least --min-facts facts with zero defects. A reference that fails against
# itself means the extractor cannot read it, and every port of it would score defects
# that are not the port's.
#
#   check-task.sh <task dir> [--min-facts 3]
#
# Prints the facts. It never prints the spec or the reference, so it is safe to run on a
# held-out task. Exit 0 ready, 1 not ready, 2 usage.
set -uo pipefail

[[ $# -ge 1 ]] || { sed -n '2,11p' "$0" | sed 's/^# \{0,1\}//' >&2; exit 2; }
dir=$1; shift
min=3
while [[ $# -gt 0 ]]; do
  case "$1" in
    --min-facts) min=$2; shift 2 ;;
    *) echo "check-task.sh: unknown argument $1" >&2; exit 2 ;;
  esac
done

repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
fail() { echo "NOT READY  $dir: $*"; exit 1; }

for f in task.toml spec.tsx reference.rs; do
  [[ -f $dir/$f ]] || fail "missing $f"
done
grep -q '^\[source\]' "$dir/task.toml" || fail "task.toml has no [source] table (provenance)"

flags=()
grep -Eq '^allow_variant_renames *= *true' "$dir/task.toml" && flags+=(--allow-variant-renames)
grep -Eq '^score_slots *= *true' "$dir/task.toml" && flags+=(--slots)

harness=(cargo run -q --manifest-path "$repo/Cargo.toml" -p mzizi-benchmark-harness --)
json=$("${harness[@]}" score --arm dioxus --candidate "$dir/reference.rs" \
  --reference "$dir/reference.rs" "${flags[@]}") || fail "the scorer failed on the reference"
facts=$(grep -o '"facts_checked":[0-9]*' <<<"$json" | cut -d: -f2)
defects=$(grep -o '"defects":[0-9]*' <<<"$json" | cut -d: -f2)
[[ -n $facts && -n $defects ]] || fail "the scorer printed no facts_checked/defects"

# The runner's own loader must accept it (enums, the renames opt-in and its reason, …): a
# dry `episode start` into a scratch directory, removed afterwards.
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT
cargo run -q --manifest-path "$repo/Cargo.toml" -p mzizi-benchmark-runner --bin mzbench -- \
  episode start --task "$dir" --arm mzizi --model-label check --out "$scratch" \
  --endpoint http://127.0.0.1:9 >/dev/null 2>"$scratch/err" ||
  fail "mzbench cannot load it: $(cat "$scratch/err")"

grep -o '"fact":"[a-z_]*"' <<<"$json" | sort | uniq -c | sed 's/^/  /'
[[ $defects -eq 0 ]] || fail "the reference scores $defects defect(s) against itself"
[[ $facts -ge $min ]] || fail "$facts scoreable fact(s), fewer than $min"
echo "READY  $dir: $facts facts, flags: ${flags[*]:-none}"
