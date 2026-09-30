# B1 — a registry's routes and methods

Write an HTTP service for a registry that holds two UI components, `badge` and `button`.
The service answers the requests below and nothing else. Paths are matched exactly and are
case-sensitive. JSON bodies are compared as JSON values, so spacing does not matter.

## Every response

Every response the service sends, whatever its status, carries the header
`x-mzizi-source: registry`.

## Routes

| Route           | `GET` responds                                                                             |
| --------------- | ------------------------------------------------------------------------------------------ |
| `/v1/health`    | `200`, `content-type: application/json`, body `{"status":"ok"}`                            |
| `/v1/ui`        | `200`, `content-type: application/json`, body `{"count":2}`                                |
| `/v1/ui/{name}` | for `badge` or `button`: `200`, `content-type: application/json`, body `{"name":"<name>"}` |
|                 | for any other name: the **not-found response**                                             |

`/v1/ui/{name}` is a route for every name, including names the registry does not hold.

## Methods, on every route

- `HEAD`: the same status and headers as `GET`, and no body.
- `OPTIONS`: `204`, the header `allow: GET, HEAD, OPTIONS`, and no body.
- Any other method: `405`, the header `allow: GET, HEAD, OPTIONS`, and no body.

## Other paths

- A path that ends with `/`, other than `/` itself, whatever the method: `308`, with
  `location` set to the same path without its trailing `/`, followed by the request's
  query string (`?` and all) if it had one. No body.
- Any other path, whatever the method: the **not-found response**.

## The not-found response

`404`, `content-type: application/json`,
`cache-control: private, no-cache, no-store, max-age=0, must-revalidate`, and the body
`{"error":"Not found"}`.
