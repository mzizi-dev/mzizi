#!/usr/bin/env bash
# benchmarks/perf/run.sh: the performance suite's runner. See README.md in this folder.
#
#   benchmarks/perf/run.sh [--check-only] [--runs N] [--out DIR] [program ...]
#
# For each program in programs/ (or only those named): build the Mzizi version with
# `mz build --out` and `cargo build --release --offline`, build the two hand-written Rust
# references (`unchecked`, `checked`), and check that all three print exactly
# programs/<name>.expected. Without --check-only it then measures each one: the median of N
# runs' wall time (default 7, after one warm-up run), the maximum RSS (with GNU time, when
# installed), the binary's size, the cold release build time, and the median `mz check`
# time. It writes DIR/perf.json and DIR/perf.md (default target/perf) and prints the table.
#
# --check-only builds and compares output, and times nothing; CI runs it so. MZ=<path> uses
# that `mz` instead of building one in release. Every package is std only and built with
# --offline. Needs Linux tools: bash 4 or later, GNU date (for %N) and coreutils.
#
# Exit status: 0 when every program's three outputs match; 1 when any differs or a program
# fails; 2 on a usage error or a missing tool; 3 when a build fails.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/../.." && pwd)"
runs=7
check_only=false
out="$root/target/perf"
only=()

while [ $# -gt 0 ]; do
  case "$1" in
    --check-only) check_only=true ;;
    --runs)
      [ $# -ge 2 ] || { echo "run.sh: --runs needs a number" >&2; exit 2; }
      runs="$2"
      shift
      ;;
    --out)
      [ $# -ge 2 ] || { echo "run.sh: --out needs a directory" >&2; exit 2; }
      out="$2"
      shift
      ;;
    -h | --help)
      sed -n '2,19p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    -*) echo "run.sh: unknown option $1" >&2; exit 2 ;;
    *) only+=("$1") ;;
  esac
  shift
done
case "$runs" in
  '' | *[!0-9]* | 0) echo "run.sh: --runs takes a positive whole number" >&2; exit 2 ;;
esac

# --- tools -------------------------------------------------------------------------------

if [ "${BASH_VERSINFO[0]}" -lt 4 ]; then
  echo "run.sh: needs bash 4 or later (associative arrays); this is $BASH_VERSION" >&2
  exit 2
fi
case "$(date +%N)" in
  '' | *[!0-9]*) echo "run.sh: needs GNU date, whose +%N gives nanoseconds" >&2; exit 2 ;;
esac

now_ns() { date +%s%N; }

# Milliseconds, to two decimals, from a nanosecond count.
ms() { awk -v ns="$1" 'BEGIN { printf "%.2f", ns / 1000000 }'; }

# The median of the numbers given, one per argument (the lower middle one for an even count).
median() { printf '%s\n' "$@" | sort -n | awk '{ v[NR] = $1 } END { print v[int((NR + 1) / 2)] }'; }

# A ratio of two numbers, to two decimals.
ratio() { awk -v a="$1" -v b="$2" 'BEGIN { printf "%.2f", a / b }'; }

# A max-RSS figure for the table: "n/a" when no GNU time measured it.
kb() { if [ "$1" = null ]; then echo "n/a"; else echo "$1"; fi; }

# A JSON string literal. Control characters are dropped, so the document is always valid.
json_str() {
  local s
  s="$(printf '%s' "$1" | tr -d '\000-\037')"
  s="${s//\\/\\\\}"
  s="${s//\"/\\\"}"
  printf '"%s"' "$s"
}

# Run a build step; a failure is exit 3, with what failed.
build() {
  if ! "$@"; then
    echo "run.sh: build failed: $*" >&2
    exit 3
  fi
}

timer=""
if [ -x /usr/bin/time ] && /usr/bin/time -f '%M' true 2> /dev/null; then
  timer=/usr/bin/time
fi

mkdir -p "$out"
out="$(cd "$out" && pwd)"
work="$out/build"
mkdir -p "$work"

# --- machine -----------------------------------------------------------------------------

cpu=""
if [ -r /proc/cpuinfo ]; then
  cpu="$(awk -F': ' '/^(model name|Model|Hardware|cpu model)[[:space:]]*:/ { print $2; exit }' /proc/cpuinfo)"
fi
if [ -z "$cpu" ] && command -v lscpu > /dev/null; then
  cpu="$(lscpu | awk -F': *' '/^Model name/ { print $2; exit }')"
fi
cpu="${cpu:-unknown ($(uname -m))}"
cores="$(getconf _NPROCESSORS_ONLN 2> /dev/null || echo 0)"
os="$(uname -srm)"
rustc_v="$(rustc --version)"
cargo_v="$(cargo --version)"
commit="$(git -C "$root" rev-parse --short=12 HEAD 2> /dev/null || echo unknown)"
when="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

# --- mz ----------------------------------------------------------------------------------

if [ -n "${MZ:-}" ]; then
  mz="$(cd "$(dirname "$MZ")" && pwd)/$(basename "$MZ")"
  [ -x "$mz" ] || { echo "run.sh: MZ=$MZ is not an executable" >&2; exit 2; }
else
  echo "run.sh: building mz (release)" >&2
  # Not --offline: mz is a workspace member, and resolving the workspace needs the crates.io
  # index on a cold cache (AGENTS.md, "Build, test, run"). It downloads no crates for mz.
  build cargo build --quiet --release --manifest-path "$root/Cargo.toml" -p mzizi-lang-compiler --bin mz
  mz="${CARGO_TARGET_DIR:-$root/target}/release/mz"
fi

programs=()
if [ ${#only[@]} -gt 0 ]; then
  for name in "${only[@]}"; do
    [ -f "$here/programs/$name.mz" ] || { echo "run.sh: no programs/$name.mz" >&2; exit 2; }
    programs+=("$name")
  done
else
  for f in "$here"/programs/*.mz; do
    programs+=("$(basename "$f" .mz)")
  done
fi

variants=(mzizi rust_unchecked rust_checked)

# --- per program -------------------------------------------------------------------------

failed=0
json_programs=()
md_rows=()

for name in "${programs[@]}"; do
  src="$here/programs/$name.mz"
  expected="$here/programs/$name.expected"
  [ -f "$expected" ] || { echo "run.sh: programs/$name.mz has no $name.expected" >&2; exit 2; }
  # The binary `mz build` makes is named from the `program` line, `_` written `-`
  # (compiler/src/run.rs, `Package::binary`), which need not be the file's name.
  program="$(awk '$1 == "program" { print $2; exit }' "$src")"
  [ -n "$program" ] || { echo "run.sh: programs/$name.mz has no \`program\` line" >&2; exit 2; }
  echo "run.sh: $name" >&2

  # Each build starts from an empty target directory, named explicitly so that
  # CARGO_TARGET_DIR cannot move it, and so its time is a cold build.
  declare -A bin build_ns
  dir="$work/$name/mzizi"
  rm -rf "$dir" "$work/$name/target-"*
  build "$mz" build "$src" --out "$dir" > /dev/null
  t0="$(now_ns)"
  build cargo build --quiet --release --offline --manifest-path "$dir/Cargo.toml" \
    --target-dir "$work/$name/target-mzizi"
  build_ns[mzizi]=$(($(now_ns) - t0))
  bin[mzizi]="$work/$name/target-mzizi/release/mz-${program//_/-}"

  for v in unchecked checked; do
    profile=release
    [ "$v" = checked ] && profile=release-checked
    t0="$(now_ns)"
    build cargo build --quiet --offline --profile "$profile" \
      --manifest-path "$here/rust/Cargo.toml" --bin "$name" --target-dir "$work/$name/target-$v"
    build_ns[rust_$v]=$(($(now_ns) - t0))
    bin[rust_$v]="$work/$name/target-$v/$profile/$name"
  done

  # Output equality: all three must print the committed .expected exactly.
  identical=true
  for v in "${variants[@]}"; do
    if ! "${bin[$v]}" > "$work/$name/$v.out"; then
      echo "run.sh: $name ($v) exited non-zero" >&2
      identical=false
    elif ! diff -u "$expected" "$work/$name/$v.out" >&2; then
      echo "run.sh: $name ($v) printed something other than programs/$name.expected" >&2
      identical=false
    fi
  done
  if [ "$identical" = true ]; then
    echo "run.sh: $name: all three outputs match $name.expected" >&2
  else
    failed=1
  fi

  # A program whose output is wrong is not timed: its numbers would mean nothing.
  if [ "$check_only" = true ] || [ "$identical" = false ]; then
    json_programs+=("$(printf '{"name":%s,"output_identical":%s,"mz_check_ms":null,"variants":{"mzizi":{"build_ms":%s},"rust_unchecked":{"build_ms":%s},"rust_checked":{"build_ms":%s}}}' \
      "$(json_str "$name")" "$identical" "$(ms "${build_ns[mzizi]}")" "$(ms "${build_ns[rust_unchecked]}")" "$(ms "${build_ns[rust_checked]}")")")
    if [ "$identical" = false ]; then
      md_rows+=("| $name | output differs from $name.expected: not timed | | | | | | | | |")
    fi
    unset bin build_ns
    continue
  fi

  # `mz check` time: the median of N runs of the shipped binary, wall clock.
  checks=()
  for _ in $(seq 1 "$runs"); do
    t0="$(now_ns)"
    "$mz" check "$src" > /dev/null
    checks+=($(($(now_ns) - t0)))
  done
  check_ms="$(ms "$(median "${checks[@]}")")"

  # Wall time: one warm-up each, then N rounds. The order of the three rotates every
  # round, so neither drift on the machine nor going first falls on one variant alone.
  declare -A walls
  for v in "${variants[@]}"; do
    "${bin[$v]}" > /dev/null
    walls[$v]=""
  done
  for round in $(seq 0 $((runs - 1))); do
    for i in 0 1 2; do
      v="${variants[$(((i + round) % 3))]}"
      t0="$(now_ns)"
      "${bin[$v]}" > /dev/null
      walls[$v]+="$(($(now_ns) - t0)) "
    done
  done

  json_variants=()
  declare -A med rss_of size_of
  for v in "${variants[@]}"; do
    # shellcheck disable=SC2086 # the run list is space-separated integers
    med[$v]="$(median ${walls[$v]})"
    rss=null
    if [ -n "$timer" ]; then
      "$timer" -f '%M' -o "$work/$name/$v.rss" "${bin[$v]}" > /dev/null
      rss="$(tail -n 1 "$work/$name/$v.rss")"
    fi
    size="$(wc -c < "${bin[$v]}" | tr -d ' ')"
    run_list="$(for n in ${walls[$v]}; do ms "$n"; echo; done | paste -sd, -)"
    json_variants+=("$(printf '%s:{"wall_ms_median":%s,"wall_ms_runs":[%s],"max_rss_kb":%s,"binary_bytes":%s,"build_ms":%s}' \
      "$(json_str "$v")" "$(ms "${med[$v]}")" "$run_list" "$rss" "$size" "$(ms "${build_ns[$v]}")")")
    rss_of[$v]="$rss"
    size_of[$v]="$size"
  done
  json_programs+=("$(printf '{"name":%s,"output_identical":%s,"mz_check_ms":%s,"variants":{%s}}' \
    "$(json_str "$name")" "$identical" "$check_ms" "$(IFS=,; echo "${json_variants[*]}")")")

  md_rows+=("$(printf '| %s | %s | %s | %s | %s | %s | %s / %s / %s | %s / %s / %s | %s / %s / %s | %s |' \
    "$name" "$(ms "${med[mzizi]}")" "$(ms "${med[rust_unchecked]}")" "$(ms "${med[rust_checked]}")" \
    "$(ratio "${med[mzizi]}" "${med[rust_unchecked]}")" "$(ratio "${med[mzizi]}" "${med[rust_checked]}")" \
    "$(kb "${rss_of[mzizi]}")" "$(kb "${rss_of[rust_unchecked]}")" "$(kb "${rss_of[rust_checked]}")" \
    "${size_of[mzizi]}" "${size_of[rust_unchecked]}" "${size_of[rust_checked]}" \
    "$(ms "${build_ns[mzizi]}")" "$(ms "${build_ns[rust_unchecked]}")" "$(ms "${build_ns[rust_checked]}")" \
    "$check_ms")")
  unset bin build_ns walls med rss_of size_of
done

# --- report ------------------------------------------------------------------------------

mode=measure
[ "$check_only" = true ] && mode=check-only
{
  printf '{"schema":"mzizi-perf/1","mode":%s,"runs":%s,' "$(json_str "$mode")" "$([ "$check_only" = true ] && echo null || echo "$runs")"
  printf '"date":%s,"commit":%s,' "$(json_str "$when")" "$(json_str "$commit")"
  printf '"machine":{"cpu":%s,"cores":%s,"os":%s,"rustc":%s,"cargo":%s,"max_rss_tool":%s},' \
    "$(json_str "$cpu")" "$cores" "$(json_str "$os")" "$(json_str "$rustc_v")" "$(json_str "$cargo_v")" \
    "$([ -n "$timer" ] && json_str "$timer" || echo null)"
  printf '"programs":[%s]}\n' "$(IFS=,; echo "${json_programs[*]}")"
} > "$out/perf.json"

if [ "$check_only" = false ]; then
  {
    echo "Measured on: $cpu, $cores cores, $os; $rustc_v; commit $commit; $when."
    echo "Wall time is the median of $runs runs, in ms. A measurement on this machine, not a claim."
    echo
    echo "| program | Mzizi (ms) | Rust unchecked (ms) | Rust checked (ms) | Mzizi ÷ unchecked | Mzizi ÷ checked | max RSS KiB (Mzizi / unchecked / checked) | binary bytes (Mzizi / unchecked / checked) | cold build ms (Mzizi / unchecked / checked) | \`mz check\` ms |"
    echo "| --- | --: | --: | --: | --: | --: | --: | --: | --: | --: |"
    printf '%s\n' "${md_rows[@]}"
  } > "$out/perf.md"
  cat "$out/perf.md"
fi
echo "run.sh: wrote $out/perf.json" >&2

if [ "$failed" -ne 0 ]; then
  echo "run.sh: at least one program's output differed from its .expected" >&2
  exit 1
fi
