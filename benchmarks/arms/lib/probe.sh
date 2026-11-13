#!/bin/sh
# Probe one backend arm's candidate with a task's probes (RFC-0009 §2.3): build the
# candidate in the arm's sandbox, serve it, send every probe, print one NDJSON line per
# fact and a summary line. Exit 0 when every fact held, 1 when one did not (or the server
# never started), 2 for a setup error. Builds `mzprobe` from this repo on first use.
#
#   probe.sh <arm-id> <task-dir> <candidate>
set -eu
[ $# -eq 3 ] || {
  echo "usage: $0 <arm-id> <task-dir> <candidate>" >&2
  exit 2
}
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../../.." && pwd)
arm=$here/../$1
[ -x "$arm/serve.sh" ] || {
  echo "probe.sh: no arm $1 with a serve.sh" >&2
  exit 2
}
cargo build -q --manifest-path "$repo/benchmarks/probe/Cargo.toml"
cand=$(cd "$(dirname "$3")" && pwd)/$(basename "$3")
exec "$repo/target/debug/mzprobe" serve "$2" -- "$arm/serve.sh" "$cand"
