#!/bin/sh
# The cpp arm's check (RFC-0009 §3), run in the arm's sandbox. See README.md and ../lib/arm-run.sh.
#
#   check.sh <candidate>     exit 0 clean, 1 the candidate has errors (printed), 2 setup error
exec "$(dirname "$0")/../lib/arm-run.sh" check "$(dirname "$0")" "$@"
