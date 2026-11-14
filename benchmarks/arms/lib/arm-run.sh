#!/bin/sh
# Run one backend arm's sandbox on one candidate (RFC-0009 §2.3, §3, §4.5).
#
#   arm-run.sh check <arm-dir> <candidate>    the arm's check; exit 0 clean, 1 errors, 2 setup
#   arm-run.sh serve <arm-dir> <candidate>    build the candidate and exec its server on $PORT
#
# Each arm's check.sh and serve.sh are one-line calls to this. The sandbox is the arm's
# Linux container image (`mzizi-arm-<id>`, built by the arm's setup.sh from its
# sandbox/Containerfile). With no `container` CLI, or MZ_ARM_RUNTIME=local, the same
# in-sandbox scripts (sandbox/run-check.sh, sandbox/run-serve.sh) run on this machine against
# the toolchain on PATH and the dependencies `sandbox/install.sh` put in the sandbox
# directory. MZ_ARM_RUNTIME=container refuses the fallback.
#
# What the container run does, so a check at episode time is offline (RFC-0009 §4.5):
#   check  --network none; the candidate is copied into a fresh directory mounted at /work,
#          so two checks never share a slot and nothing a candidate writes survives.
#   serve  the port is published on 127.0.0.1 only (a published port needs a network), the
#          build writes to a `.arm-work` directory beside the candidate, and the server is
#          killed after ARM_SERVE_TTL seconds (default 300). mzprobe stops a server with
#          SIGKILL, which the container CLI cannot forward, so the TTL is what reaps it.
set -eu

usage() {
  echo "usage: $0 check|serve <arm-dir> <candidate>" >&2
  exit 2
}
[ $# -eq 3 ] || usage
mode=$1
armdir=$(cd "$2" && pwd)
candidate=$3
case $mode in check | serve) ;; *) usage ;; esac
[ -f "$candidate" ] && [ -r "$candidate" ] || {
  echo "arm-run.sh: not a readable file: $candidate" >&2
  exit 2
}

id=$(basename "$armdir")
image="mzizi-arm-$id"
base=$(basename "$candidate")
candidate_dir=$(cd "$(dirname "$candidate")" && pwd)

runtime=${MZ_ARM_RUNTIME:-auto}
if [ "$runtime" = auto ]; then
  if command -v container >/dev/null 2>&1; then runtime=container; else runtime=local; fi
fi
case $runtime in container | local) ;; *)
  echo "arm-run.sh: MZ_ARM_RUNTIME must be auto, container or local" >&2
  exit 2
  ;;
esac

if [ "$mode" = check ]; then
  work=$(mktemp -d "${TMPDIR:-/tmp}/mzizi-arm-$id.XXXXXX")
  trap 'rm -rf "$work"' EXIT
else
  work=$candidate_dir/.arm-work
  rm -rf "$work"
  mkdir -p "$work"
  [ -n "${PORT:-}" ] || {
    echo "arm-run.sh: serve needs PORT" >&2
    exit 2
  }
fi
# `cat >`, not `cp -p`: a fresh mtime, so no build cache mistakes two same-size candidates.
cat -- "$candidate" >"$work/$base"

if [ "$runtime" = local ]; then
  script=$armdir/sandbox/run-$mode.sh
  cd "$work"
  if [ "$mode" = serve ]; then
    ARM_HOME=$armdir/sandbox exec sh "$script" "$base"
  fi
  # Not exec, so the EXIT trap removes the work directory.
  ARM_HOME=$armdir/sandbox sh "$script" "$base"
  exit $?
fi

container image inspect "$image" >/dev/null 2>&1 || {
  echo "arm-run.sh: image $image not found; run $armdir/setup.sh once" >&2
  exit 2
}
limits="--cpus ${ARM_CPUS:-4} --memory ${ARM_MEMORY:-3G}"
if [ "$mode" = check ]; then
  # shellcheck disable=SC2086
  container run --progress none --rm --network none $limits -v "$work:/work" -w /work \
    "$image" sh /opt/arm/run-check.sh "$base"
  exit $?
fi
# shellcheck disable=SC2086
exec container run --progress none --rm --name "mzizi-arm-$id-$$" $limits \
  -p "127.0.0.1:$PORT:$PORT" -e "PORT=$PORT" -e "ARM_SERVE_TTL=${ARM_SERVE_TTL:-300}" \
  -v "$work:/work" -w /work "$image" sh /opt/arm/run-serve.sh "$base"
