#!/bin/sh
# The Python arm's check (RFC-0009 §3): `pyright` in its default `standard` mode, with the
# arm's virtualenv as the environment the candidate's imports resolve against. Run in the
# directory holding the candidate, given its file name. It prints pyright's own diagnostics
# with the directory prefix removed, and its count line. Exit 0 no errors (warnings
# allowed); 1 the candidate has errors; 2 setup error.
set -u
ARM_HOME=${ARM_HOME:-/opt/arm}
f=$1
pyright=$ARM_HOME/node_modules/pyright/index.js
[ -f "$pyright" ] && [ -x "$ARM_HOME/.venv/bin/python" ] || {
  echo "run-check.sh: $ARM_HOME is not installed (run install.sh)" >&2
  exit 2
}
printf '{ "include": ["%s"], "venvPath": "%s", "venv": ".venv", "typeCheckingMode": "standard" }\n' \
  "$f" "$ARM_HOME" >pyrightconfig.json
out=$(node "$pyright" 2>&1)
code=$?
# Paths are shown as the candidate's file name, not the directory it was checked in.
out=$(printf '%s\n' "$out" | sed -e "s#$PWD/##g")
case $code in
  0)
    printf '%s\n' "$out"
    exit 0
    ;;
  1)
    printf '%s\n' "$out"
    exit 1
    ;;
esac
echo "run-check.sh: pyright exited $code (setup error):" >&2
printf '%s\n' "$out" >&2
exit 2
