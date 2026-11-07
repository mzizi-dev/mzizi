#!/bin/sh
# Build one backend arm's sandbox image: `mzizi-arm-<id>`, from the arm's
# sandbox/Containerfile, with the arm's sandbox directory as the build context. The only
# step that uses the network (RFC-0009 §4.5: caches warmed before the first episode).
#
#   build-image.sh <arm-dir>
set -eu
[ $# -eq 1 ] || {
  echo "usage: $0 <arm-dir>" >&2
  exit 2
}
armdir=$(cd "$1" && pwd)
id=$(basename "$armdir")
command -v container >/dev/null 2>&1 || {
  echo "build-image.sh: the \`container\` CLI is not installed; for the local runtime run $armdir/sandbox/install.sh" >&2
  exit 2
}
container build -t "mzizi-arm-$id" -f "$armdir/sandbox/Containerfile" "$armdir/sandbox"
