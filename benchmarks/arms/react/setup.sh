#!/usr/bin/env bash
# Install the React arm's pinned toolchain into sandbox/node_modules, once, before a
# benchmark run (RFC-0009 §4.5: caches warmed before the first episode). This is the only
# step that uses the network. `npm ci` installs exactly sandbox/package-lock.json and fails
# rather than change it. Never run by CI: no Node toolchain is a CI requirement (RFC-0009 §9).
set -euo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
command -v npm >/dev/null || { echo "setup.sh: npm not found" >&2; exit 2; }
cd "$here/sandbox"
npm ci --ignore-scripts --no-audit --no-fund
node node_modules/typescript/bin/tsc --version
