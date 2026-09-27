#!/usr/bin/env bash
# Phase 0 benchmark pilot, open-weight arm: fetch llama.cpp, download pinned
# Qwen2.5-Coder GGUFs with SHA256 verification, and launch llama-server.
#
# Usage:
#   benchmarks/openweight/setup.sh fetch     # llama.cpp prebuilt CPU binary
#   benchmarks/openweight/setup.sh models    # download + verify all three GGUFs
#   benchmarks/openweight/setup.sh launch    # start llama-server (detached)
#   benchmarks/openweight/setup.sh all       # fetch + models + launch (default)
#
# Idempotent: every step checks what is already on disk (and verifies it)
# before doing any work. Nothing is written inside the repository.
#
# Environment overrides:
#   LLAMA_DIR   (default /opt/llama.cpp)
#   MODEL_DIR   (default /opt/models)
#   MODELS      (default "1.5b 3b 7b"; which sizes `models` downloads)
#   SERVE_MODEL (default 7b; which size `launch` serves)
#   THREADS     (default 4)
#   CTX         (default 16384)
#   PORT        (default 8080)
#   LOG         (default /opt/models/llama-server.log)
set -euo pipefail

# ---- pinned versions -------------------------------------------------------
LLAMA_TAG="b11206" # ggml-org/llama.cpp release; binary reports commit 2b129ccfa
LLAMA_ASSET="llama-${LLAMA_TAG}-bin-ubuntu-x64.tar.gz"
LLAMA_URL="https://github.com/ggml-org/llama.cpp/releases/download/${LLAMA_TAG}/${LLAMA_ASSET}"
LLAMA_SHA256="aea9ff64167ea473bf5cf463f07b42beac16c857cdffce57c7ed9cc323ea4da5"

# Official Qwen GGUF repos on Hugging Face, pinned by SHA256 (the LFS oid).
declare -A MODEL_REPO=(
  [1.5b]="Qwen/Qwen2.5-Coder-1.5B-Instruct-GGUF"
  [3b]="Qwen/Qwen2.5-Coder-3B-Instruct-GGUF"
  [7b]="Qwen/Qwen2.5-Coder-7B-Instruct-GGUF"
)
declare -A MODEL_SHA256=(
  [1.5b]="cc324af070c2ecbfd324a30884d2f951a7ff756aba85cb811a6ec436933bb046"
  [3b]="724fb256bec1ff062b2f65e4569e871ad2e95ab2a3989723d1769c54294730b7"
  [7b]="509287f78cb4d4cf6b3843734733b914b2c158e43e22a7f4bf5e963800894d3c"
)

LLAMA_DIR="${LLAMA_DIR:-/opt/llama.cpp}"
MODEL_DIR="${MODEL_DIR:-/opt/models}"
MODELS="${MODELS:-1.5b 3b 7b}"
SERVE_MODEL="${SERVE_MODEL:-7b}"
THREADS="${THREADS:-4}"
CTX="${CTX:-16384}"
PORT="${PORT:-8080}"
LOG="${LOG:-${MODEL_DIR}/llama-server.log}"
BIN_DIR="${LLAMA_DIR}/llama-${LLAMA_TAG}"

model_file() { echo "qwen2.5-coder-$1-instruct-q4_k_m.gguf"; }

verify_sha() { # file expected
  local got
  got="$(sha256sum "$1" | awk '{print $1}')"
  [[ "$got" == "$2" ]]
}

fetch() {
  if [[ -x "${BIN_DIR}/llama-server" ]]; then
    echo "llama.cpp ${LLAMA_TAG} already at ${BIN_DIR}"
    return
  fi
  mkdir -p "${LLAMA_DIR}"
  local tgz="${LLAMA_DIR}/${LLAMA_ASSET}"
  curl -sSLf --retry 3 -o "$tgz" "$LLAMA_URL"
  if ! verify_sha "$tgz" "$LLAMA_SHA256"; then
    echo "SHA256 mismatch for ${LLAMA_ASSET}" >&2
    rm -f "$tgz"
    exit 1
  fi
  tar -xzf "$tgz" -C "${LLAMA_DIR}"
  rm -f "$tgz"
  "${BIN_DIR}/llama-server" --version 2>&1 | grep -E '^version'
}

models() {
  mkdir -p "${MODEL_DIR}"
  local s f
  for s in ${MODELS}; do
    f="${MODEL_DIR}/$(model_file "$s")"
    if [[ -f "$f" ]] && verify_sha "$f" "${MODEL_SHA256[$s]}"; then
      echo "ok (verified) $f"
      continue
    fi
    curl -sSLf --retry 3 -C - -o "$f" \
      "https://huggingface.co/${MODEL_REPO[$s]}/resolve/main/$(model_file "$s")"
    if ! verify_sha "$f" "${MODEL_SHA256[$s]}"; then
      echo "SHA256 mismatch for $f" >&2
      exit 1
    fi
    echo "downloaded (verified) $f"
  done
}

launch() {
  if curl -sSf "http://127.0.0.1:${PORT}/health" >/dev/null 2>&1; then
    echo "llama-server already answering on 127.0.0.1:${PORT}"
    pgrep -af llama-server || true
    return
  fi
  local f="${MODEL_DIR}/$(model_file "${SERVE_MODEL}")"
  [[ -f "$f" ]] || {
    echo "missing $f; run '$0 models'" >&2
    exit 1
  }
  # -np 1: one slot, so sequential requests reuse the previous request's KV
  # prefix (llama-server's default cache_prompt). 4 threads = 4 physical cores.
  setsid nohup "${BIN_DIR}/llama-server" \
    -m "$f" \
    --host 127.0.0.1 --port "${PORT}" \
    -c "${CTX}" -t "${THREADS}" -tb "${THREADS}" \
    -np 1 --jinja \
    >"${LOG}" 2>&1 </dev/null &
  local pid=$!
  echo "started llama-server pid ${pid}, log ${LOG}"
  local i
  for i in $(seq 1 120); do
    if curl -sSf "http://127.0.0.1:${PORT}/health" >/dev/null 2>&1; then
      echo "healthy after ${i}s"
      return
    fi
    sleep 1
  done
  echo "llama-server did not become healthy; see ${LOG}" >&2
  exit 1
}

case "${1:-all}" in
  fetch) fetch ;;
  models) models ;;
  launch) launch ;;
  all)
    fetch
    models
    launch
    ;;
  *)
    echo "usage: $0 [fetch|models|launch|all]" >&2
    exit 2
    ;;
esac
