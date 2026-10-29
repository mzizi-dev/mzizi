#!/bin/sh
# Install the Python arm's pinned dependencies into this directory (the image's /opt/arm, or
# the arm's sandbox/ for the local runtime): a virtualenv from requirements.txt, and pyright
# from package-lock.json with `npm ci`.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
cd "$here"
python3 -m venv .venv
.venv/bin/python -m pip install --no-cache-dir --disable-pip-version-check -r requirements.txt
npm ci --ignore-scripts --no-audit --no-fund
.venv/bin/python --version
node --version
node node_modules/pyright/index.js --version
