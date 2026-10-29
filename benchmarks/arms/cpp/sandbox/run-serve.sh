#!/bin/sh
# Serve the C++ arm's candidate on $PORT: compile and link it (link errors surface here, as
# `start_failed`, RFC-0009 §3), then exec it. The candidate reads PORT itself and listens
# on 0.0.0.0 (the guide says so).
set -eu
ARM_HOME=${ARM_HOME:-/opt/arm}
f=$1
clang++ -std=c++20 -O0 -isystem "$ARM_HOME/include" "$f" -o server -pthread >&2
exec timeout "${ARM_SERVE_TTL:-300}" ./server
