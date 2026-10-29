#!/bin/sh
# Warm the Go build cache for the standard-library packages a backend candidate uses, so
# a check does not compile them (RFC-0009 §4.5). Nothing is downloaded: the standard
# library ships with the toolchain.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
export GOCACHE=$here/gocache GOFLAGS=-mod=mod GOPROXY=off GOTOOLCHAIN=local
tmp=$(mktemp -d)
cd "$tmp"
printf 'module warm\n\ngo 1.24\n' >go.mod
cat >main.go <<'EOF'
package main

import (
	"encoding/json"
	"fmt"
	"log"
	"net/http"
	"os"
	"strings"
)

func main() {
	_ = json.NewEncoder(os.Stdout)
	_ = strings.TrimSuffix
	fmt.Println(http.StatusOK)
	log.Println("warm")
}
EOF
go build ./...
go vet ./...
go version
rm -rf "$tmp"
