#!/usr/bin/env bash
set -u
cd /workspace/mzizi
R="$1"
M=qwen2.5-coder-7b-instruct-q4km
run() { # task arm seed out
  echo "$(date -u +%FT%TZ) START $1 $2 seed=$3"
  ./target/release/mzbench run --task "benchmarks/tasks/$1" --arm "$2" --endpoint http://127.0.0.1:8080 \
    --model-label "$M" --seed "$3" --temperature 0.2 --max-iters 5 --out "$4"
  echo "$(date -u +%FT%TZ) END $1 $2 seed=$3 exit=$?"
}
for seed in 1 2 3; do for task in button badge; do for arm in mzizi dioxus; do run $task $arm $seed "$R/scored"; done; done; done
for arm in mzizi dioxus; do run nyuchi-changelog-renderer $arm 1 "$R/unscored"; done
echo "$(date -u +%FT%TZ) ALL DONE"
