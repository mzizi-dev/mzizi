#!/bin/sh
# Serve the TypeScript arm's candidate on $PORT: compile it with the same tsc and tsconfig,
# then run server.mjs, which serves the candidate's default export (a Hono app) with
# @hono/node-server on 0.0.0.0. Execs the server, so stopping this process stops it.
set -eu
ARM_HOME=${ARM_HOME:-/opt/arm}
f=$1
ln -sfn "$ARM_HOME/node_modules" node_modules
printf '{ "type": "module" }\n' >package.json
printf '{ "extends": "%s/tsconfig.json", "compilerOptions": { "noEmit": false, "outDir": "out" }, "include": ["%s"] }\n' \
  "$ARM_HOME" "$f" >tsconfig.json
# Type errors do not stop the emit: a candidate that passed the check is what is served.
node "$ARM_HOME/node_modules/typescript/bin/tsc" -p tsconfig.json --pretty false >&2 || true
js="out/${f%.ts}.js"
[ -f "$js" ] || {
  echo "run-serve.sh: tsc emitted nothing for $f" >&2
  exit 1
}
exec timeout "${ARM_SERVE_TTL:-300}" node "$ARM_HOME/server.mjs" "$PWD/$js"
