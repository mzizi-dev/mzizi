#!/bin/sh
# Serve the Go arm's candidate on $PORT: build it, then exec it. The candidate reads PORT
# itself and listens on 0.0.0.0 (the guide says so).
set -eu
ARM_HOME=${ARM_HOME:-/opt/arm}
export GOCACHE=$ARM_HOME/gocache GOFLAGS=-mod=mod GOPROXY=off GOTOOLCHAIN=local CGO_ENABLED=0
printf 'module candidate\n\ngo 1.24\n' >go.mod
go build -o server . >&2
exec timeout "${ARM_SERVE_TTL:-300}" ./server
