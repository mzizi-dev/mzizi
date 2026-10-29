// Serve a candidate's default export, a Hono app, on $PORT (the arm's run-serve.sh).
import { pathToFileURL } from "node:url";
import { serve } from "@hono/node-server";

const mod = await import(pathToFileURL(process.argv[2]).href);
if (!mod.default || typeof mod.default.fetch !== "function") {
  console.error("server.mjs: the candidate has no default export with a fetch method (a Hono app)");
  process.exit(1);
}
serve({ fetch: mod.default.fetch, port: Number(process.env.PORT), hostname: "0.0.0.0" });
