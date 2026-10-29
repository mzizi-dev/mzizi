#!/bin/sh
# Start B4's Python reference on $PORT (standard library only), then exec it so that stopping
# this process stops it.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
exec python3 -I "$here/server.py"
