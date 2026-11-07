#!/bin/sh
# The TypeScript arm's check (RFC-0009 §3): `tsc --noEmit --pretty false` with `strict: true`
# (tsconfig.json). Run in the directory holding the candidate, given its file name. It
# prints tsc's own diagnostics, paths relative to the candidate's directory, and nothing
# else. Exit 0 no type errors; 1 the candidate has type errors; 2 setup error.
set -u
ARM_HOME=${ARM_HOME:-/opt/arm}
f=$1
tsc=$ARM_HOME/node_modules/typescript/bin/tsc
[ -f "$tsc" ] || {
  echo "run-check.sh: $ARM_HOME/node_modules is not installed (run install.sh)" >&2
  exit 2
}
# The candidate imports `hono`: resolve it from the arm's node_modules.
ln -sfn "$ARM_HOME/node_modules" node_modules
printf '{ "extends": "%s/tsconfig.json", "include": ["%s"] }\n' "$ARM_HOME" "$f" >tsconfig.json
out=$(node "$tsc" -p tsconfig.json --noEmit --pretty false 2>&1)
code=$?
if [ $code -eq 0 ]; then
  [ -n "$out" ] && printf '%s\n' "$out"
  exit 0
fi
# A diagnostic against the candidate is its own; anything else is a broken install.
if printf '%s\n' "$out" | grep -q "^$f("; then
  printf '%s\n' "$out"
  exit 1
fi
echo "run-check.sh: tsc failed without a diagnostic against the candidate (setup error):" >&2
printf '%s\n' "$out" >&2
exit 2
