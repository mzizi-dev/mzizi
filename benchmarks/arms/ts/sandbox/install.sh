#!/bin/sh
# Install the TypeScript arm's pinned dependencies into this directory (the image's /opt/arm,
# or the arm's sandbox/ for the local runtime). `npm ci` installs exactly package-lock.json
# and fails rather than change it.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
cd "$here"
npm ci --ignore-scripts --no-audit --no-fund
node node_modules/typescript/bin/tsc --version
node --version
