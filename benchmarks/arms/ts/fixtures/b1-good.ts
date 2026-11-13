// A known-good B1 solution in TypeScript with Hono, written to check the TypeScript arm
// (README.md, "Verifying the arm"). Never shown to an author.
import { Hono } from "hono";
import type { Context } from "hono";

const ALLOW = "GET, HEAD, OPTIONS";
const NO_STORE = "private, no-cache, no-store, max-age=0, must-revalidate";
const HELD = ["badge", "button"];

const app = new Hono({ strict: true });

app.use(async (c, next) => {
  await next();
  c.header("x-mzizi-source", "registry");
});

app.use(async (c, next) => {
  const url = new URL(c.req.url);
  if (url.pathname.length > 1 && url.pathname.endsWith("/")) {
    return c.redirect(url.pathname.slice(0, -1) + url.search, 308);
  }
  await next();
});

const notFound = (c: Context) => c.json({ error: "Not found" }, 404, { "cache-control": NO_STORE });

app.get("/v1/health", (c) => c.json({ status: "ok" }));
app.get("/v1/ui", (c) => c.json({ count: 2 }));
app.get("/v1/ui/:name", (c) => {
  const name = c.req.param("name");
  return HELD.includes(name) ? c.json({ name }) : notFound(c);
});

for (const path of ["/v1/health", "/v1/ui", "/v1/ui/:name"]) {
  app.options(path, (c) => c.body(null, 204, { allow: ALLOW }));
  app.all(path, (c) => c.body(null, 405, { allow: ALLOW }));
}

app.notFound(notFound);

export default app;
