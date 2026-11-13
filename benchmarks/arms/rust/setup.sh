#!/bin/sh
# Build the rust arm's sandbox image, once per machine, before any episode (RFC-0009 §4.5).
# The only step that uses the network. Never run by CI. For the local runtime, run
# sandbox/install.sh instead.
exec "$(dirname "$0")/../lib/build-image.sh" "$(dirname "$0")" "$@"
