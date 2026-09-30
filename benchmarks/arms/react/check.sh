#!/usr/bin/env bash
# React arm type check. See README.md in this directory.
#
#   check.sh <candidate.tsx>
#
# Places the candidate at sandbox/src/component.tsx and runs the sandbox's pinned
# `tsc --noEmit --pretty false` with `strict: true` (sandbox/tsconfig.json), the check an
# ordinary TypeScript project runs in CI (RFC-0009 §3). It prints tsc's own diagnostics,
# with the slot's path shown as the candidate's file name, and nothing else. Nothing is
# fetched: sandbox/node_modules must already exist (../setup.sh installs it once).
#
# Exit codes:
#   0  no type errors
#   1  the candidate has type errors (printed)
#   2  usage or setup error (message on stderr)
set -euo pipefail

usage() {
  echo "usage: $0 <candidate.tsx>" >&2
  exit 2
}

[[ $# -eq 1 ]] || usage
candidate=$1
[[ -f $candidate && -r $candidate ]] || {
  echo "check.sh: not a readable file: $candidate" >&2
  exit 2
}
command -v flock >/dev/null || {
  echo "check.sh: required tool not found: flock" >&2
  exit 2
}
command -v node >/dev/null || {
  echo "check.sh: required tool not found: node" >&2
  exit 2
}

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
sandbox=$here/sandbox
tsc=$sandbox/node_modules/typescript/bin/tsc
[[ -f $tsc ]] || {
  echo "check.sh: $sandbox/node_modules is not installed; run $here/setup.sh once" >&2
  exit 2
}
slot=$sandbox/src/component.tsx
display=$(basename -- "$candidate")
display_sed=$(printf '%s' "$display" | sed -e 's/[\\&#]/\\&/g')

# One check at a time per sandbox: two concurrent runs would overwrite each other's slot.
exec 9>"$sandbox/check.lock"
flock 9

cleanup() { rm -f -- "$slot"; }
trap cleanup EXIT
cat -- "$candidate" >"$slot"

set +e
out=$(cd "$sandbox" && node "$tsc" --noEmit --pretty false -p tsconfig.json 2>&1)
code=$?
set -e

# tsc exits 0 clean, and 1 or 2 when it reports diagnostics. A diagnostic against the slot
# is the candidate's; anything else (a broken install, a config error) is setup.
if [[ $code -eq 0 ]]; then
  [[ -n $out ]] && printf '%s\n' "$out" | sed -e "s#src/component\.tsx#${display_sed}#g"
  exit 0
fi
if grep -q '^src/component\.tsx(' <<<"$out"; then
  printf '%s\n' "$out" | sed -e "s#src/component\.tsx#${display_sed}#g"
  exit 1
fi
echo "check.sh: tsc failed without a diagnostic against the candidate (setup error):" >&2
printf '%s\n' "$out" >&2
exit 2
