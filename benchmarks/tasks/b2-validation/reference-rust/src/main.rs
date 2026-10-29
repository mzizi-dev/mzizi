//! B2's Rust reference (benchmarks/tasks/b2-validation/spec.md), in plain axum.

use axum::extract::Request;
use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router, middleware};
use serde_json::json;
use std::collections::HashMap;

const ALLOW: &str = "GET, HEAD, OPTIONS";

/// Decode one query component: `+` is a space, `%XX` is a byte, and a malformed escape is
/// kept as written. Invalid UTF-8 after decoding is replaced, as the spec leaves it open.
fn decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() && hex(b[i + 1]).is_some() && hex(b[i + 2]).is_some() => {
                out.push(hex(b[i + 1]).unwrap_or(0) * 16 + hex(b[i + 2]).unwrap_or(0));
                i += 2;
            }
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex(c: u8) -> Option<u8> {
    (c as char).to_digit(16).map(|d| d as u8)
}

/// The first occurrence of each name wins.
fn params(query: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for pair in query.split('&').filter(|p| !p.is_empty()) {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        map.entry(decode(k)).or_insert_with(|| decode(v));
    }
    map
}

/// `-`? then one or more ASCII digits, fitting a signed 64-bit integer; anything else is absent.
fn int(value: Option<&String>) -> Option<i64> {
    let v = value?;
    let digits = v.strip_prefix('-').unwrap_or(v);
    if digits.is_empty() || !digits.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    v.parse().ok()
}

fn problem(message: &str) -> Response {
    (StatusCode::BAD_REQUEST, Json(json!({ "error": message }))).into_response()
}

async fn list(req: Request) -> Response {
    let p = params(req.uri().query().unwrap_or(""));
    let limit = int(p.get("limit"));
    let offset = int(p.get("offset"));
    if let Some(l) = limit
        && !(1..=100).contains(&l)
    {
        return problem("limit must be between 1 and 100");
    }
    if let Some(o) = offset
        && o < 0
    {
        return problem("offset must be 0 or more");
    }
    Json(json!({ "node": p.get("node"), "limit": limit, "offset": offset })).into_response()
}

fn not_found() -> Response {
    (StatusCode::NOT_FOUND, Json(json!({ "error": "Not found" }))).into_response()
}

/// OPTIONS, and every method a route does not declare.
async fn other_method(method: Method) -> Response {
    let status = if method == Method::OPTIONS {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::METHOD_NOT_ALLOWED
    };
    (status, [(header::ALLOW, ALLOW)]).into_response()
}

/// A path no route matches: the trailing-slash redirect, else the not-found response.
async fn fallback(req: Request) -> Response {
    let path = req.uri().path();
    if path.len() > 1 && path.ends_with('/') {
        let mut location = path.trim_end_matches('/').to_string();
        if let Some(q) = req.uri().query() {
            location.push('?');
            location.push_str(q);
        }
        return (
            StatusCode::PERMANENT_REDIRECT,
            [(header::LOCATION, location)],
        )
            .into_response();
    }
    not_found()
}

async fn source(mut res: Response) -> Response {
    res.headers_mut()
        .insert("x-mzizi-source", HeaderValue::from_static("registry"));
    res
}

fn app() -> Router {
    Router::new()
        .route("/v1/ui", get(list).fallback(other_method))
        .fallback(fallback)
        .layer(middleware::map_response(source))
}

#[tokio::main]
async fn main() {
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .expect("bind the port");
    println!("listening on http://127.0.0.1:{port}");
    axum::serve(listener, app()).await.expect("serve");
}
