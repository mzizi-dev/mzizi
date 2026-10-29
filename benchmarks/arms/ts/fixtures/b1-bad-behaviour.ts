// Known-bad, for the probes: type-checks clean, but serves only the three GET routes. It has
// no x-mzizi-source header, no HEAD or OPTIONS or 405 handling, and no trailing-slash
// redirect, so most of B1's probes must fail on it.
import { Hono } from "hono";

const app = new Hono();
app.get("/v1/health", (c) => c.json({ status: "ok" }));
app.get("/v1/ui", (c) => c.json({ count: 2 }));
app.get("/v1/ui/:name", (c) => c.json({ name: c.req.param("name") }));

export default app;
