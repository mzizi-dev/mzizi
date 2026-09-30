#!/usr/bin/env bash
# Verify every code block in a prompt guide against the checker that arm really runs.
#
# ```mz blocks go through `mz check --agent` and `mz contract --agent`; ```rust and ```tsx
# blocks through the arm's own check.sh (benchmarks/arms/<arm>/check.sh, where <arm> is the
# guide's file name before `-guide.md`). A block is expected to pass (exit 0; for `mz`,
# contract too), unless its first line contains "WRONG ON PURPOSE". Then the check must
# exit 1, and its output must equal the guide's next ```text block exactly, so the output a
# guide shows the model is what the checker prints. The file name is taken from that text
# block (`tag.rs`, `"file":"tag.mz"`); `mz`'s "ms" timing is compared as 0.
#
# The Rust and React checks need their arm's toolchain installed (arms/*/README.md). They
# run only when a guide is being changed, never in CI (RFC-0009 §9).
#
# usage: benchmarks/prompts/verify-guide.sh [guide.md] [out-dir]
set -u

root="$(cd "$(dirname "$0")/../.." && pwd)"
guide="${1:-$root/benchmarks/prompts/mzizi-guide.md}"
out="${2:-$(mktemp -d)}"
mkdir -p "$out"
arm="$(basename "$guide")"
arm="${arm%-guide.md}"

cargo build -q --manifest-path "$root/compiler/Cargo.toml" --bin mz || exit 2
mz="$root/target/debug/mz"

# Split the guide into block-<n>.<lang>, recording each block's language in order.
awk -v out="$out" '
  /^```(mz|rust|tsx|text)$/ && !inblock {
    n++; inblock = 1; lang = substr($0, 4)
    file = sprintf("%s/block-%03d.%s", out, n, lang); printf "" > file
    print n, lang > (out "/index"); next
  }
  /^```$/ && inblock { inblock = 0; close(file); next }
  inblock { print > file }
' "$guide"
[ -s "$out/index" ] || { echo "no code blocks found in $guide"; exit 2; }

# check <lang> <dir> <file name>: print the checker's stdout, return its exit status.
check() {
  local text rc
  case "$1" in
    mz) text=$(cd "$2" && "$mz" check --agent "$3") ;;
    *) text=$("$root/benchmarks/arms/$arm/check.sh" "$2/$3") ;;
  esac
  rc=$?
  printf '%s\n' "$text" | sed -E 's/"ms":[0-9]+/"ms":0/'
  return "$rc"
}

status=0
mapfile -t blocks <"$out/index"
for ((i = 0; i < ${#blocks[@]}; i++)); do
  read -r n lang <<<"${blocks[$i]}"
  [ "$lang" = text ] && continue
  block=$(printf '%s/block-%03d.%s' "$out" "$n" "$lang")
  name=$(basename "$block")
  if head -n 1 "$block" | grep -q "WRONG ON PURPOSE"; then
    read -r tn tlang <<<"${blocks[$((i + 1))]:-0 none}"
    expected=$(printf '%s/block-%03d.text' "$out" "$tn")
    if [ "$tlang" != text ]; then
      echo "FAIL  $name  wrong on purpose, but no \`\`\`text block follows it"
      status=1
      continue
    fi
    file=$(grep -oE '[A-Za-z0-9_-]+\.(mz|rs|tsx)' "$expected" | head -n 1)
    dir="$out/wrong-$n"
    mkdir -p "$dir"
    cp "$block" "$dir/$file"
    got=$(check "$lang" "$dir" "$file")
    rc=$?
    if [ "$rc" -eq 1 ] && [ "$got" = "$(cat "$expected")" ]; then
      echo "ok    $name  check=1, output matches the guide ($file)"
    else
      echo "FAIL  $name  check=$rc (expected 1), or its output differs from the guide:"
      diff <(printf '%s' "$got") "$expected" | head -n 20
      status=1
    fi
  else
    ext=$lang
    [ "$lang" = rust ] && ext=rs
    dir="$out/ok-$n"
    mkdir -p "$dir"
    cp "$block" "$dir/candidate.$ext"
    check "$lang" "$dir" "candidate.$ext" >"$dir/check.out"
    rc=$?
    crc=0
    if [ "$lang" = mz ]; then
      (cd "$dir" && "$mz" contract --agent candidate.mz) >"$dir/contract.out"
      crc=$?
    fi
    if [ "$rc" -eq 0 ] && [ "$crc" -eq 0 ]; then
      echo "ok    $name  check=0$([ "$lang" = mz ] && echo ' contract=0')"
    else
      echo "FAIL  $name  check=$rc contract=$crc (expected 0): $dir"
      status=1
    fi
  fi
done

echo "blocks and output in $out"
exit "$status"
