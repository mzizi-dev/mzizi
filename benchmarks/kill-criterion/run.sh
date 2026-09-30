#!/usr/bin/env bash
# The kill-criterion benchmark driver: every task in a task set, on every arm, for every
# seed, against one model endpoint (mzbench Mode 1). See README.md in this directory and
# ../READINESS.md for what must be true before its output counts as the Phase 0 number, and
# design/RFC-0009-comparison-benchmark.md §6-§7 for the rule and what is published.
#
#   run.sh --tasks <task-set dir> --out <results dir> --model-label <label> \
#          [--endpoint http://127.0.0.1:8080] [--arms "mzizi dioxus leptos"] \
#          [--family ui-port] [--seeds "1 2 3 4 5"] [--temperature 0.7] [--max-iters 5]
#          [--min-facts 3] [--dry-run]
#
# --family picks the input (RFC-0009 §2): ui-port hands every arm spec.tsx, as the pilots
# did; ui-spec hands it spec.md, and is the gating family. Each arm must list the family in
# its arm.toml.
#
# A PLAN.md must exist in <out> or in the results directory above it before a real run
# starts (RFC-0009 §7.2: the plan is registered before the first episode); --dry-run does
# not need it. `mzbench plan` drafts one from the same arguments. manifest.json records the
# context size the server reports (server_n_ctx, from llama.cpp's /props), or null when the
# endpoint does not say.
#
# The task set is a path argument and nothing else (MIGRATION.md §4.2): a held-out set
# checked out from the private repository is passed exactly like benchmarks/tasks. This
# script never copies, prints or commits a task's text. It writes <out>/manifest.json, which
# identifies each task by name and the SHA-256 of each of its files, so that a result from a
# held-out set can be published with scores while the task texts stay private until the
# set is retired (RFC-0004 §4.2). At the end it writes <out>/bundle.sha256, the SHA-256 of
# the sorted `sha256sum` listing of every file under <out>/episodes: the raw-bundle hash a
# held-out run publishes on the day, which anyone can recompute once the bundle is published.
# `mzbench bundle-hash <out>` computes it (runner/src/plan.rs, tested against this same
# coreutils pipeline: find episodes -type f | LC_ALL=C sort | xargs sha256sum | sha256sum).
#
# Exit status: 0 when every episode finished (clean or not), 1 when any aborted, 2 on usage.
set -uo pipefail

usage() {
  sed -n '2,33p' "$0" | sed 's/^# \{0,1\}//' >&2
  exit 2
}

tasks="" out="" model="" endpoint="http://127.0.0.1:8080"
arms="mzizi dioxus leptos" family="ui-port" seeds="1 2 3 4 5" temperature="0.7" max_iters="5" min_facts=3 dry=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --tasks) tasks=$2; shift 2 ;;
    --out) out=$2; shift 2 ;;
    --model-label) model=$2; shift 2 ;;
    --endpoint) endpoint=$2; shift 2 ;;
    --arms) arms=$2; shift 2 ;;
    --family) family=$2; shift 2 ;;
    --seeds) seeds=$2; shift 2 ;;
    --temperature) temperature=$2; shift 2 ;;
    --max-iters) max_iters=$2; shift 2 ;;
    --min-facts) min_facts=$2; shift 2 ;;
    --dry-run) dry=1; shift ;;
    *) usage ;;
  esac
done
[[ -n $tasks && -n $out && -n $model ]] || usage
[[ -d $tasks ]] || { echo "run.sh: not a directory: $tasks" >&2; exit 2; }
if [[ $dry -eq 0 && ! -f $out/PLAN.md && ! -f $(dirname "$out")/PLAN.md ]]; then
  echo "run.sh: no PLAN.md in $out or $(dirname "$out"); register the plan before" \
    "the first episode (RFC-0009 §7.2); \`mzbench plan\` drafts one" >&2
  exit 2
fi

repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
task_dirs=()
for d in "$tasks"/*/; do
  [[ -f $d/task.toml ]] && task_dirs+=("${d%/}")
done
[[ ${#task_dirs[@]} -gt 0 ]] || { echo "run.sh: no task.toml under $tasks" >&2; exit 2; }

# Every task must pass the same check a task author runs, before any model time is spent.
for d in "${task_dirs[@]}"; do
  "$repo/benchmarks/kill-criterion/check-task.sh" "$d" --min-facts "$min_facts" >/dev/null || {
    echo "run.sh: $d fails check-task.sh; run it for the reason" >&2
    exit 2
  }
done

mkdir -p "$out"
# The server's context size is a setting of the run (README.md, "The pre-registered settings").
n_ctx=$(curl -fsS --max-time 5 "$endpoint/props" 2>/dev/null | grep -o '"n_ctx": *[0-9]*' |
  head -n1 | grep -o '[0-9]*$')
commit=$(git -C "$repo" rev-parse HEAD 2>/dev/null || echo unknown)
dirty=$(git -C "$repo" status --porcelain 2>/dev/null | grep -qv '^??' && echo true || echo false)
{
  printf '{\n  "started": "%s",\n  "repo_commit": "%s",\n  "repo_dirty": %s,\n' \
    "$(date -u +%FT%TZ)" "$commit" "$dirty"
  printf '  "model": "%s",\n  "endpoint": "%s",\n  "arms": "%s",\n  "family": "%s",\n' \
    "$model" "$endpoint" "$arms" "$family"
  printf '  "seeds": "%s",\n' "$seeds"
  printf '  "temperature": %s,\n  "max_iters": %s,\n  "server_n_ctx": %s,\n  "tasks": [\n' \
    "$temperature" "$max_iters" "${n_ctx:-null}"
  sep=""
  for d in "${task_dirs[@]}"; do
    printf '%s    {"dir": "%s"' "$sep" "$(basename "$d")"
    for f in task.toml spec.tsx spec.md reference.rs; do
      [[ -f $d/$f ]] && printf ', "%s": "%s"' "$f" "$(sha256sum "$d/$f" | cut -d' ' -f1)"
    done
    printf '}'
    sep=$',\n'
  done
  printf '\n  ]\n}\n'
} >"$out/manifest.json"

(cd "$repo" && cargo build -q --release -p mzizi-benchmark-runner -p mzizi-benchmark-harness \
  -p mzizi-lang-compiler) || exit 2
mzbench="$repo/target/release/mzbench"

aborted=0
for seed in $seeds; do
  for d in "${task_dirs[@]}"; do
    for arm in $arms; do
      echo "$(date -u +%FT%TZ) START $(basename "$d") $arm seed=$seed"
      if [[ $dry -eq 1 ]]; then continue; fi
      if ! "$mzbench" run --task "$d" --arm "$arm" --endpoint "$endpoint" \
        --model-label "$model" --family "$family" --seed "$seed" --temperature "$temperature" \
        --max-iters "$max_iters" --out "$out/episodes"; then
        aborted=$((aborted + 1))
      fi
      echo "$(date -u +%FT%TZ) END $(basename "$d") $arm seed=$seed"
    done
  done
done

if [[ $dry -eq 0 ]]; then
  "$mzbench" summarize "$out/episodes" >"$out/summary.md"
  "$mzbench" bundle-hash "$out" >"$out/bundle.sha256"
  echo "summary: $out/summary.md ($aborted aborted episode(s)); bundle $(cat "$out/bundle.sha256")"
fi
[[ $aborted -eq 0 ]]
