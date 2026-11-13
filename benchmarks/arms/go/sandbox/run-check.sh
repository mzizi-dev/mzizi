#!/bin/sh
# The Go arm's check (RFC-0009 §3): `go build ./...`, then `go vet ./...`. Run in the
# directory holding the candidate, given its file name. Prints the tools' own output
# (build errors first; vet findings only when the build passed) and nothing else.
# Exit 0 clean; 1 build errors or vet findings; 2 setup error.
set -u
ARM_HOME=${ARM_HOME:-/opt/arm}
f=$1
export GOCACHE=$ARM_HOME/gocache GOFLAGS=-mod=mod GOPROXY=off GOTOOLCHAIN=local CGO_ENABLED=0
printf 'module candidate\n\ngo 1.24\n' >go.mod
out=$(go build ./... 2>&1)
code=$?
if [ $code -ne 0 ]; then
  printf '%s\n' "$out"
  printf '%s\n' "$out" | grep -q "\./$f:[0-9]" && exit 1
  echo "run-check.sh: go build failed without a diagnostic against the candidate (setup error)" >&2
  exit 2
fi
out=$(go vet ./... 2>&1)
code=$?
[ -n "$out" ] && printf '%s\n' "$out"
if [ $code -ne 0 ]; then
  printf '%s\n' "$out" | grep -q "\./$f:[0-9]" && exit 1
  echo "run-check.sh: go vet failed without a diagnostic against the candidate (setup error)" >&2
  exit 2
fi
exit 0
