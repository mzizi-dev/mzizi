# Writing one Rust service with axum

You write one Rust (`.rs`) file, the `src/main.rs` of a binary crate, holding one HTTP
service built on axum 0.8 and tokio. Your file is checked with `cargo check --locked`;
errors fail it, warnings print and do not. It is then built, run, and answers real HTTP
requests. If a construct is not shown here, use what you know of Rust, axum and serde.

## File shape

- One file, edition 2024. The crates are `axum` (0.8.9), `tokio` (1.53.3, features
  `rt-multi-thread`, `macros`, `net`), `serde` (1.0.229, with `derive`) and `serde_json`
  (1.0.151), plus `std`. No other crate exists, and you cannot add one.
- `#[tokio::main] async fn main()` builds a `Router`, binds a listener to `0.0.0.0` on the
  port in the `PORT` environment variable, and calls `axum::serve(listener, app).await`.
- Use `use axum::{...}` at the top. `main` must not return before the server stops.

## What is available

**Routes.** `Router::new().route("/v1/items/{id}", get(handler))`. A path parameter is
written `{id}` (0.8 syntax; `:id` is an error at start-up), and a catch-all is `{*rest}`.
Combine methods with `get(a).post(b).delete(c)`, import them from `axum::routing`. A
request no route matches goes to `.fallback(handler)`; without one it gets an empty `404`.

**Extractors** are handler parameters, in this order: any number of those that read the
request head, then at most one that reads the body.

| Parameter                                  | Gives                                           |
| ------------------------------------------ | ----------------------------------------------- |
| `Path(id): Path<String>`                   | a path parameter (a tuple for several)          |
| `Query(q): Query<HashMap<String, String>>` | the query parameters                            |
| `method: Method` (`axum::http`)            | the method                                      |
| `headers: HeaderMap`                       | the request headers                             |
| `uri: Uri`                                 | the request URI: `uri.path()`, `uri.query()`    |
| `req: Request`                             | the whole request (`axum::extract::Request`)    |
| `Json(v): Json<T>` with `T: Deserialize`   | the JSON body; a bad body is an extractor error |

A handler is `async fn(...) -> impl IntoResponse`. A `Query<T>` fails with `400` when a
value does not fit `T`; use `HashMap<String, String>` to read the raw text.

**Responses.** These implement `IntoResponse`: `String` and `&'static str` (as
`text/plain; charset=utf-8`), `StatusCode`, `Json(value)` (as `application/json`), and
tuples that start with a status or headers and end with a body:
`(StatusCode::NOT_FOUND, [(header::CACHE_CONTROL, "no-store")], Json(v))`. `()` is an
empty `200`; `StatusCode::NO_CONTENT` is an empty `204`. Use `axum::http::header` for
header names or write a lower-case `&str`. A `308` redirect is
`(StatusCode::PERMANENT_REDIRECT, [(header::LOCATION, location)])`.

**JSON.** `serde_json::json!({ "id": id, "unit": null })` builds a `Value` and `Json` sends
it. `#[derive(Serialize, Deserialize)]` on a struct gives it a JSON shape; `Option<T>`
serialises as `null` when `None`.

**Middleware.** `.layer(axum::middleware::map_response(f))` where `f` is
`async fn(Response) -> Response` adds to every response, `.fallback` included, when it is
the last call on the router. `middleware::from_fn(f)` with `async fn(Request, Next) ->
Response` sees the request first.

**Defaults.** `get` routes also answer `HEAD`, without the body. Another method on a
route's path is answered `405` with an `allow` header and an empty body. `OPTIONS` is
answered the same `405`. A path with a trailing slash does not match its route and gets
the `404`. No response has a `cache-control` or `x-` header unless you add one.

## Naming rules

- Functions, variables and modules are `snake_case`; types and traits are `PascalCase`;
  constants are `SCREAMING_SNAKE_CASE`.
- A handler's name is yours to choose; only its method and path are observable.
- JSON keys are whatever the task says: string keys in `json!`, or `#[serde(rename =
"...")]` on a field.

## Reading the checker's output

A file wrong on purpose:

```rust
// WRONG ON PURPOSE: two mistakes.
use axum::{Router, routing::get};

async fn item() -> String {
    let total: u32 = "3";
    format!("{}", count)
}

#[tokio::main]
async fn main() {
    let app = Router::new().route("/v1/items/{id}", get(item));
}
```

`cargo check` prints each diagnostic as rustc renders it, with `help:` and `note:` lines,
and nothing when the file is clean:

```text
error[E0425]: cannot find value `count` in this scope
 --> tag.rs:6:19
  |
6 |     format!("{}", count)
  |                   ^^^^^ not found in this scope


error[E0308]: mismatched types
 --> tag.rs:5:22
  |
5 |     let total: u32 = "3";
  |                ---   ^^^ expected `u32`, found `&str`
  |                |
  |                expected due to this


Some errors have detailed explanations: E0308, E0425.

For more information about an error, try `rustc --explain E0308`.
```

- `--> file:line:column` locates the error; the code (`E0308`) can be explained with
  `rustc --explain E0308`. A `help:` line is often the exact repair. A `warning:` is not a
  failure, but it says something is unused or unreachable.
- Fix every error in one pass: each is a separate mistake, though one early mistake can
  cause later ones.
- The checker proves that the program type-checks and borrows correctly. It cannot see
  that a route answers the right status, so re-read the task for every status, header and
  body before you finish.

## A worked example

A weather-station service. It is not one of the tasks.

```rust
//! Two stations, their readings, and a JSON 404.
use std::collections::HashMap;

use axum::extract::{Path, Query};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router, middleware};
use serde::Serialize;
use serde_json::json;

const STATIONS: [&str; 2] = ["harare", "bulawayo"];

#[derive(Serialize)]
struct Station {
    id: String,
    readings: u32,
    unit: Option<String>,
}

fn problem(status: StatusCode, error: &str) -> Response {
    (status, Json(json!({ "error": error }))).into_response()
}

async fn station_list(Query(q): Query<HashMap<String, String>>) -> Response {
    let readings = match q.get("limit") {
        None => 2,
        Some(raw) => match raw.parse::<u32>() {
            Ok(n) if n >= 1 => n,
            _ => return problem(StatusCode::BAD_REQUEST, "limit must be 1 or more"),
        },
    };
    Json(Station { id: "all".into(), readings, unit: None }).into_response()
}

async fn station_item(Path(id): Path<String>, Query(q): Query<HashMap<String, String>>) -> Response {
    if !STATIONS.contains(&id.as_str()) {
        return problem(StatusCode::NOT_FOUND, &format!("no {id}"));
    }
    let body = Station { id, readings: 24, unit: q.get("unit").cloned() };
    ([(header::CACHE_CONTROL, "max-age=60")], Json(body)).into_response()
}

async fn station_reset(Path(_id): Path<String>) -> StatusCode {
    StatusCode::NO_CONTENT
}

async fn not_found() -> Response {
    problem(StatusCode::NOT_FOUND, "Not found")
}

async fn tag_service(mut res: Response) -> Response {
    res.headers_mut().insert("x-service", HeaderValue::from_static("stations"));
    res
}

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/v1/stations", get(station_list))
        .route("/v1/stations/{id}", get(station_item))
        .route("/v1/stations/{id}/reset", post(station_reset))
        .fallback(not_found)
        .layer(middleware::map_response(tag_service));
    let port: u16 = std::env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(8080);
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await.expect("bind");
    axum::serve(listener, app).await.expect("serve");
}
```
