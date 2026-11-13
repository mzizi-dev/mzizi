#!/bin/sh
# The C++ arm has nothing to install: its libraries are the vendored headers in include/
# (NOTICE). This verifies them against the pinned hashes and prints the compiler version.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
cd "$here/include"
sha256sum -c - <<'EOF'
9021001a719768893a8a6e4051c56107690e8b8e49b41e5f903534fa19ae9afb  httplib.h
f8b557b73311b275473f5b5fe25d3c37dace7942a3126ef509ff90f50936b7a6  nlohmann/json.hpp
EOF
clang++ --version | head -n 1
