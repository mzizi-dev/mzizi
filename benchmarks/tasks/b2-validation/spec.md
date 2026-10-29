# B2 — a listing with validated query parameters

Write an HTTP service with one route, `GET /v1/ui`, that reads three optional query
parameters, `node`, `limit` and `offset`, and echoes what it understood. Paths are matched
exactly and are case-sensitive. JSON bodies are compared as JSON values, so spacing does
not matter.

## Every response

Every response the service sends, whatever its status, carries the header
`x-mzizi-source: registry`.

## Reading the query string

The query string is split on `&`, and each piece on its first `=` (a piece with no `=` has
the empty string as its value). Names and values are decoded as
`application/x-www-form-urlencoded`: `+` is a space, and `%XX` (two hexadecimal digits) is
that byte. A `%` that is not followed by two hexadecimal digits is kept as written. If a name
appears more than once, **only its first occurrence counts**. A name the service does not
know is ignored.

## `GET /v1/ui`

Each parameter is read like this:

| Parameter | Present when                                                      | Otherwise |
| --------- | ----------------------------------------------------------------- | --------- |
| `node`    | the name is in the query string, even with an empty value         | absent    |
| `limit`   | the value is an integer: an optional `-`, then one or more digits | absent    |
| `offset`  | the value is an integer: an optional `-`, then one or more digits | absent    |

An integer here is only that: a value such as `abc`, `0x10`, `1e2`, `1.5`, `+5`, `7`
(spaces) or the empty string is **not** an integer, so the parameter is **absent**. That is
not an error. A value that has only digits but does not fit in a signed 64-bit integer is not
an integer either.

Then the service checks the values it has, in this order:

1. `limit`, if present, must be between `1` and `100` inclusive. Otherwise the response is
   `400`, `content-type: application/json`, body `{"error":"limit must be between 1 and 100"}`.
2. `offset`, if present, must be `0` or more. Otherwise the response is `400`,
   `content-type: application/json`, body `{"error":"offset must be 0 or more"}`.

If both are out of range, the `limit` response wins.

Otherwise the response is `200`, `content-type: application/json`, and the body is the
object `{"node": <node>, "limit": <limit>, "offset": <offset>}` with the decoded `node` text
or `null` if absent, and `limit` and `offset` as JSON numbers or `null` if absent.

## Methods, on `/v1/ui`

- `HEAD`: the same status and headers as `GET`, and no body.
- `OPTIONS`: `204`, the header `allow: GET, HEAD, OPTIONS`, and no body.
- Any other method: `405`, the header `allow: GET, HEAD, OPTIONS`, and no body.

## Other paths

- A path that ends with `/`, other than `/` itself, whatever the method: `308`, with
  `location` set to the same path without its trailing `/`, followed by the request's query
  string (`?` and all) if it had one. No body.
- Any other path, whatever the method: `404`, `content-type: application/json`, body
  `{"error":"Not found"}`.
