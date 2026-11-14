#!/bin/sh
# Verify one backend arm against B1 (RFC-0009 §2.3): a known-good solution in the arm's
# language must pass its check and every probe, and two known-bad ones must fail. Needs the
# arm's sandbox (setup.sh) and a model-free machine: nothing here calls a model.
#
#   self-test.sh <arm-id> [task-dir]       task-dir defaults to benchmarks/tasks/b1-routing
#
# Fixtures, in benchmarks/arms/<id>/fixtures/:
#   b1-good.<ext>           check exits 0, every probe holds
#   b1-bad-type.<ext>       check exits 1 (the language's checker rejects it)
#   b1-bad-behaviour.<ext>  check exits 0, but at least one probe fails
# Exit 0 when all of that held, 1 when any did not, 2 for a setup error.
set -u
[ $# -ge 1 ] || {
  echo "usage: $0 <arm-id> [task-dir]" >&2
  exit 2
}
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../../.." && pwd)
arm=$here/../$1
task=${2:-$repo/benchmarks/tasks/b1-routing}
ext=$(sed -n 's/^extension = "\(.*\)"$/\1/p' "$arm/arm.toml")
[ -n "$ext" ] || {
  echo "self-test.sh: no extension in $arm/arm.toml" >&2
  exit 2
}
fx=$arm/fixtures
bad=0
say() { printf '%s\n' "$*"; }

say "== $1: check b1-good.$ext (want exit 0)"
"$arm/check.sh" "$fx/b1-good.$ext"
code=$?
say "exit $code"
[ $code -eq 0 ] || bad=1

say "== $1: probe b1-good.$ext (want every fact to hold)"
out=$("$here/probe.sh" "$1" "$task" "$fx/b1-good.$ext")
code=$?
say "$out" | tail -n 1
say "exit $code"
[ $code -eq 0 ] || bad=1

say "== $1: check b1-bad-type.$ext (want exit 1)"
"$arm/check.sh" "$fx/b1-bad-type.$ext"
code=$?
say "exit $code"
[ $code -eq 1 ] || bad=1

say "== $1: check b1-bad-behaviour.$ext (want exit 0)"
"$arm/check.sh" "$fx/b1-bad-behaviour.$ext" >/dev/null
code=$?
say "exit $code"
[ $code -eq 0 ] || bad=1

say "== $1: probe b1-bad-behaviour.$ext (want facts to fail, exit 1)"
out=$("$here/probe.sh" "$1" "$task" "$fx/b1-bad-behaviour.$ext")
code=$?
say "$out" | tail -n 1
say "exit $code"
[ $code -eq 1 ] || bad=1

if [ $bad -eq 0 ]; then say "self-test: $1 ok"; else say "self-test: $1 FAILED"; fi
exit $bad
