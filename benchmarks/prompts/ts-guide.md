# Writing one TypeScript service with Hono

You write one TypeScript (`.ts`) file holding one HTTP service, built on Hono. Your file is
type-checked with `tsc --noEmit` under `strict: true`; type errors fail it, and nothing
else is checked. It is then compiled, served on a port, and answers real HTTP requests. If a
construct is not shown here, use what you know of TypeScript, Hono and Node.

## File shape

- One file. `import { Hono } from "hono"` (a named import; there is no default export) and
  type-only imports as `import type { Context } from "hono"`. Hono is the only library: no
  other package is installed. Node's built-in modules (`node:...`) are available.
- Build one `const app = new Hono()` and end the file with `export default app;`. Do not
  start a server, read a port or call `serve`: the build serves your default export.
- Imports are ES modules (`module: NodeNext`); `require` is not available. Write no
  `.js` or `.ts` extension for `hono`.

## What is available

**Routing.** `app.get(path, handler)`, and the same for `post`, `put`, `patch`, `delete`,
`options`; `app.all(path, handler)` for every method; `app.on(["GET", "POST"], path,
handler)` for a list. A path is `/`-separated static segments and `:name` parameters
(`/v1/items/:id`). Handlers run in the order registered; the first to return a response
wins. `new Hono({ strict: true })` makes `/a` and `/a/` different paths; by default they
are the same.

**The context `c`.**

| Call                                      | Gives                                             |
| ----------------------------------------- | ------------------------------------------------- |
| `c.req.param("id")`                       | a path parameter, as `string`                     |
| `c.req.query("limit")`                    | one query parameter, `string \| undefined`        |
| `c.req.method`, `c.req.path`, `c.req.url` | the request line (`url` is absolute)              |
| `c.req.header("name")`                    | a request header, `string \| undefined`           |
| `c.json(value, status?, headers?)`        | a JSON response, `content-type: application/json` |
| `c.text(string, status?, headers?)`       | a text response                                   |
| `c.body(null, status?, headers?)`         | a response with no body                           |
| `c.redirect(location, status?)`           | a redirect, `302` unless a status is given        |
| `c.header("name", "value")`               | sets a header on the response being built         |

Status codes are numbers, but Hono types them as literal unions: `c.json({ error: "no" },
404)` is fine, while a status held in a `number` variable is a type error. Give such a
variable a literal type (`400 | 404`). A handler may be `async`.

**Reading the request.** `await c.req.json()` parses a JSON body and rejects when it is
malformed, so wrap it in `try`/`catch`; its result is `any`, so check it before use.
`await c.req.text()` reads the raw body. `c.req.queries("k")` gives every value of a
repeated parameter. `new URL(c.req.url)` gives `pathname` and `search` (the query string,
with its `?`, or `""`) when you need the request exactly as sent. Path and query values
arrive percent-decoded; header names are matched case-insensitively.

**Headers on a response.** Pass them as the third argument (`c.json(body, 200, { "x-a":
"1" })`) or call `c.header` before you return. A response made by `c.redirect`,
`c.json` or `c.body` is the one `await next()` hands back to middleware, so middleware
may still add headers to it.

**Middleware** is `app.use(async (c, next) => { ... await next(); ... })`. Code after
`await next()` sees the response, and `c.header(...)` there adds a header to it. Returning
a response before `next()` stops the chain. `app.use` without a path applies to every
request.

**Types** you may import from `hono`: `Context` (the type of `c`), `Next` (middleware's
second argument), `MiddlewareHandler`, and `Hono`. Name a middleware with
`const m: MiddlewareHandler = async (c, next) => { await next(); }`, then `app.use(m)`.
`app.route("/v1", sub)` mounts another `Hono` app under a prefix.

**Defaults.** A request no handler answers gets Hono's plain-text `404 Not Found`;
replace it with `app.notFound((c) => c.json({ error: "Not found" }, 404))`. A `HEAD`
request runs the matching `get` handler and drops the body. `OPTIONS` and a method with no
handler on a path that exists are not answered for you: they are `404` unless you write
handlers. Uncaught errors become a `500` with a plain-text body; replace that with
`app.onError((err, c) => c.json({ error: "Internal server error" }, 500))`. A response
body is sent exactly as given: Hono adds no `allow`, `location` or `cache-control`
header, and no redirect for a missing or extra trailing slash. A path with `:name` matches
one segment, never an empty one, and never one holding `/`.

## Naming rules

- Variables and functions are `camelCase`; types are `PascalCase`; constants may be
  `UPPER_SNAKE`. JSON field names are whatever the task says, written as string keys.
- A handler's parameter is `c`; the route's `:name` is read with `c.req.param("name")`.
- Annotate nothing the compiler can infer. `strict` rejects implicit `any`, so a parameter
  you declare needs a type, and `undefined` must be handled before use.

## Reading the checker's output

A file wrong on purpose:

```ts
// WRONG ON PURPOSE: two mistakes.
import { Hono } from "hono";

const app = new Hono();

app.get("/v1/items/:id", (c) => {
  const id: number = c.req.param("id");
  return c.json({ id, total: count });
});

export default app;
```

`tsc --noEmit --pretty false` prints one line per error, and nothing when the file is clean:

```text
tag.ts(7,9): error TS2322: Type 'string' is not assignable to type 'number'.
tag.ts(8,30): error TS2304: Cannot find name 'count'.
```

- `file(line,column)` locates the error; the code (`TS2322`) and message follow. Some
  messages continue on indented lines below, which belong to the error above.
- Fix every error in one pass: each line is a separate mistake. Here, read the parameter
  as a string (`const id = c.req.param("id")`), and define `count` or remove it.
- Codes you will meet: `TS2322` a value of the wrong type; `TS2304` an unknown name;
  `TS2307` a module that cannot be found (only `hono` and `node:` modules exist);
  `TS7006` a parameter with no type; `TS18048` a value that may be `undefined`;
  `TS2345` an argument of the wrong type; `TS2554` the wrong number of arguments.
- The checker proves types only. It cannot see that a route answers the right status, so
  re-read the task for every status, header and body before you finish.

## A worked example

A weather-station service. It is not one of the tasks.

```ts
// Two stations, their readings, and a JSON 404.
import { Hono } from "hono";
import type { Context } from "hono";

const STATIONS = ["harare", "bulawayo"];

type Problem = { error: string };

const app = new Hono({ strict: true });

app.use(async (c, next) => {
  await next();
  c.header("x-service", "stations");
});

const problem = (c: Context, status: 400 | 404, error: string) =>
  c.json({ error } satisfies Problem, status);

app.get("/v1/stations", (c) => {
  const raw = c.req.query("limit");
  if (raw === undefined) {
    return c.json({ id: "all", readings: 2, unit: null });
  }
  const limit = Number(raw);
  if (Number.isInteger(limit) && limit >= 1) {
    return c.json({ id: "all", readings: limit, unit: null });
  }
  return problem(c, 400, "limit must be 1 or more");
});

app.get("/v1/stations/:id", (c) => {
  const id = c.req.param("id");
  if (STATIONS.includes(id)) {
    c.header("cache-control", "max-age=60");
    return c.json({ id, readings: 24, unit: c.req.query("unit") ?? null });
  }
  return problem(c, 404, `no ${id}`);
});

app.post("/v1/stations/:id/reset", (c) => c.body(null, 204));

app.notFound((c) => problem(c, 404, "Not found"));

export default app;
```
