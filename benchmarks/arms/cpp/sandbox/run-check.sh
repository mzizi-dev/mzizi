#!/bin/sh
# The C++ arm's check (RFC-0009 §3): `clang++ -std=c++20 -fsyntax-only -Wall -Wextra
# -fno-color-diagnostics`, with the vendored headers on the system include path (-isystem),
# so their own warnings are not the candidate's. Run in the directory holding the
# candidate, given its file name. It prints clang's output, paths as the candidate's file
# name. Exit 0 no errors (warnings allowed); 1 errors; 2 setup error.
set -u
ARM_HOME=${ARM_HOME:-/opt/arm}
f=$1
[ -f "$ARM_HOME/include/httplib.h" ] || {
  echo "run-check.sh: $ARM_HOME/include is missing" >&2
  exit 2
}
out=$(clang++ -std=c++20 -fsyntax-only -Wall -Wextra -fno-color-diagnostics \
  -isystem "$ARM_HOME/include" "$f" 2>&1)
code=$?
if [ $code -eq 0 ]; then
  [ -n "$out" ] && printf '%s\n' "$out"
  exit 0
fi
printf '%s\n' "$out"
printf '%s\n' "$out" | grep -q "^$f:[0-9]" && exit 1
echo "run-check.sh: clang++ failed without a diagnostic against the candidate (setup error)" >&2
exit 2
