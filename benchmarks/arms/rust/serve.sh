#!/bin/sh
# Serve one rust candidate on $PORT, for probing (RFC-0009 §2.3):
#
#   mzprobe serve <task-dir> -- benchmarks/arms/rust/serve.sh <candidate>
#
# A candidate that passed check.sh but does not build or start is `start_failed`.
exec "$(dirname "$0")/../lib/arm-run.sh" serve "$(dirname "$0")" "$@"
