# B3 — a gateway's read routes, retired routes and CORS

Write an HTTP service that mirrors the read side of a small registry gateway. Paths are
matched exactly and are case-sensitive. JSON bodies are compared as JSON values, so spacing
does not matter.

## Every response

Every response the service sends, whatever its status, carries these four headers:

| Header                         | Value                |
| ------------------------------ | -------------------- |
| `x-mzizi-source`               | `registry`           |
| `access-control-allow-origin`  | `*`                  |
| `access-control-allow-methods` | `GET, HEAD, OPTIONS` |
| `access-control-allow-headers` | `content-type`       |

## The listing

`GET /v1/ui` answers with this JSON, exactly:

```json
{
  "components": [
    { "name": "badge", "node": "N2" },
    { "name": "button", "node": "N2" }
  ],
  "meta": { "total": 2, "source": "fixture" }
}
```

## Routes

| Route                          | `GET` responds                                                                                                                               |
| ------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------- |
| `/v1/health`                   | `200`, `content-type: application/json`, `cache-control: no-cache`, body `{"status":"ok"}`                                                   |
| `/v1/ui`                       | `200`, `content-type: application/json`, `cache-control: public, max-age=300`, the listing above                                             |
| `/v1/ui/{name}`                | for `badge` or `button`: `200`, `content-type: application/json`, `cache-control: public, max-age=300`, body `{"name":"<name>","node":"N2"}` |
|                                | for any other name: the **not-found response**                                                                                               |
| `/v1/components`, `/v1/themes` | retired: `410`, `content-type: application/json`, `cache-control: no-store`, body `{"error":"Gone"}`                                         |
| `/v1/versions`                 | not available: `503`, `content-type: application/json`, `cache-control: no-store`, body `{"error":"Database not configured"}`                |

`/v1/ui/{name}` is a route for every name, including names the registry does not hold. The
retired and unavailable routes answer only the paths in the table: `/v1/components/badge` is
an unknown path.

## Methods, on every route above

- `HEAD`: the same status and headers as `GET`, and no body.
- `OPTIONS`: `204`, the header `allow: GET, HEAD, OPTIONS`, and no body.
- Any other method: `405`, the header `allow: GET, HEAD, OPTIONS`, and no body.

## Other paths

- A path that ends with `/`, other than `/` itself, whatever the method: `308`, with
  `location` set to the same path without its trailing `/`, followed by the request's query
  string (`?` and all) if it had one. No body.
- Any other path, whatever the method: the **not-found response**.

## The not-found response

`404`, `content-type: application/json`,
`cache-control: private, no-cache, no-store, max-age=0, must-revalidate`, and the body
`{"error":"Not found"}`.
