# B4 — a version history that remembers

Write an HTTP service that records the versions of two UI components, `badge` and `button`,
and lists them back. The history lives in memory only: it starts empty each time the service
starts. Paths are matched exactly and are case-sensitive. JSON bodies are compared as JSON
values, so spacing does not matter.

## Every response

Every response the service sends, whatever its status, carries the header
`x-mzizi-source: registry`.

## Routes

Both routes are for `/v1/ui/{name}/versions`, where `{name}` is `badge` or `button`. For any
other name, the response is the **not-found response**, whatever the method or the body.

### `GET /v1/ui/{name}/versions`

`200`, `content-type: application/json`, and the body
`{"component":"<name>","versions":[...],"count":<n>}`. `versions` holds every version recorded
for that component, as strings, **in the order they were recorded**, and `count` is how many.
A component with no versions has `"versions":[]` and `"count":0`.

### `POST /v1/ui/{name}/versions`

The request body is a JSON object with a `version` that is a non-empty string, for example
`{"version":"1.0.0"}`. Versions are compared as exact text: `1.0.0` and `1.0.00` are
different versions.

- If the body is not that, the response is `400`, `content-type: application/json`, and the
  body `{"error":"Invalid body"}`. Nothing is recorded.
- If the component already has that version, the response is `409`,
  `content-type: application/json`, and the body `{"error":"Version already recorded"}`.
  Nothing is recorded.
- Otherwise the version is recorded at the end of that component's history, and the response
  is `201`, `content-type: application/json`, the header `location: /v1/ui/{name}/versions`,
  and the body `{"component":"<name>","version":"<version>","count":<n>}`, where `<n>` is how
  many versions the component has now.

Each component has its own history: a version recorded for `badge` says nothing about
`button`. The checks run in this order: the name, then the body, then the duplicate.

## Methods

- `HEAD`: on a `GET` route, the same status and headers as `GET`, and no body.
- `OPTIONS`: `204`, the header `allow: GET, HEAD, POST, OPTIONS`, and no body.
- Any other method: `405`, the header `allow: GET, HEAD, POST, OPTIONS`, and no body.

## Other paths

Any path other than the one route, whatever the method (a trailing slash included), gets the
**not-found response**.

## The not-found response

`404`, `content-type: application/json`, and the body `{"error":"Not found"}`.
