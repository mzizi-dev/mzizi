#!/bin/sh
# Serve the Python arm's candidate on $PORT: uvicorn runs the candidate module's `app` (a
# FastAPI instance) on 0.0.0.0. Execs the server, so stopping this process stops it.
set -eu
ARM_HOME=${ARM_HOME:-/opt/arm}
f=$1
exec timeout "${ARM_SERVE_TTL:-300}" "$ARM_HOME/.venv/bin/python" -m uvicorn \
  --app-dir "$PWD" "$(basename "$f" .py):app" --host 0.0.0.0 --port "$PORT" --log-level warning
