# B5 — a service that fails well

Write an HTTP service whose job is to answer bad requests and internal failures in a
predictable way. Paths are matched exactly and are case-sensitive. JSON bodies are compared
as JSON values, so spacing does not matter.

## Every response

Every response the service sends, whatever its status, carries the header
`x-mzizi-source: registry`. No response body ever contains anything but what this document
says it contains: no stack trace, no file name, no description of an internal cause.

## Paths first

Before anything else, the path is made canonical. If the path has an empty segment (`//`)
or ends with `/` (and is not `/` itself), the response is `308`, with `location` set to the
path with its empty segments removed and its trailing `/` dropped, followed by the request's
query string (`?` and all) if it had one. No body. The method does not matter. The location
always starts with exactly one `/`, so a path such as `//evil.com/` is answered with
`location: /evil.com`, never with a location that begins `//`.

A path that is `/` alone, or has no empty segment and no trailing `/`, is matched against the
routes below. A path that matches none gets the **not-found response**.

## Routes

| Route          | Method | Responds                                                                          |
| -------------- | ------ | --------------------------------------------------------------------------------- |
| `/v1/health`   | `GET`  | `200`, `content-type: application/json`, body `{"status":"ok"}`                   |
| `/v1/fail`     | `GET`  | `500`, `content-type: application/json`, body `{"error":"Internal server error"}` |
| `/v1/messages` | `POST` | see below                                                                         |

`/v1/fail` stands for an endpoint whose work breaks inside the service. Its response never
says why.

### `POST /v1/messages`

The checks run in this order, and the first that fails is the response:

1. The request's `content-type` must be `application/json`: the media type is compared
   without regard to case, and parameters such as `; charset=utf-8` are allowed. Otherwise
   (including no `content-type` at all) the response is `415`, `content-type: application/json`,
   and the body `{"error":"Expected application/json"}`.
2. The body must be at most **1024 bytes**. Otherwise `413`, `content-type: application/json`,
   body `{"error":"Payload too large"}`.
3. The body must be valid JSON. Otherwise `400`, `content-type: application/json`, body
   `{"error":"Malformed JSON"}`.
4. The JSON must be an object with a `text` that is a non-empty string. Otherwise `400`,
   `content-type: application/json`, body `{"error":"Invalid message"}`.
5. Otherwise `201`, `content-type: application/json`, and the body `{"text":"<text>"}` with
   the text from the request.

## Methods

- `HEAD`: on a `GET` route, the same status and headers as `GET`, and no body.
- `OPTIONS`: `204`, an `allow` header, and no body.
- Any other method: `405`, an `allow` header, and no body.

The `allow` header is `GET, HEAD, OPTIONS` on `/v1/health` and `/v1/fail`, and
`POST, OPTIONS` on `/v1/messages`.

## The not-found response

`404`, `content-type: application/json`, and the body `{"error":"Not found"}`.
