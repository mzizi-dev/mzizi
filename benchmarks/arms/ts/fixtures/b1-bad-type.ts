// Known-bad, for the check: B1's good solution with a type error. `check.sh` must exit 1.
import { Hono } from "hono";

const count: number = "2";
const app = new Hono();
app.get("/v1/ui", (c) => c.json({ count }));

export default app;
