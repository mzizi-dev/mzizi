//! B3's Rust reference (benchmarks/tasks/b3-mirror/spec.md), in plain axum.

use axum::extract::{Path, Request};
use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router, middleware};
use serde_json::json;

const ALLOW: &str = "GET, HEAD, OPTIONS";
const NO_STORE: &str = "private, no-cache, no-store, max-age=0, must-revalidate";
const PUBLIC: &str = "public, max-age=300";
/// The listing, shared with the Mzizi reference's copy (fixtures/ui.json).
const LISTING: &str = include_str!("../../fixtures/ui.json");

fn with_cache(status: StatusCode, cache: &'static str, body: serde_json::Value) -> Response {
    (status, [(header::CACHE_CONTROL, cache)], Json(body)).into_response()
}

async fn health() -> Response {
    with_cache(StatusCode::OK, "no-cache", json!({ "status": "ok" }))
}

async fn list() -> Response {
    (
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::CACHE_CONTROL, PUBLIC),
        ],
        LISTING,
    )
        .into_response()
}

async fn item(Path(name): Path<String>) -> Response {
    match name.as_str() {
        "badge" | "button" => with_cache(
            StatusCode::OK,
            PUBLIC,
            json!({ "name": name, "node": "N2" }),
        ),
        _ => not_found(),
    }
}

async fn gone() -> Response {
    with_cache(StatusCode::GONE, "no-store", json!({ "error": "Gone" }))
}

async fn versions() -> Response {
    with_cache(
        StatusCode::SERVICE_UNAVAILABLE,
        "no-store",
        json!({ "error": "Database not configured" }),
    )
}

fn not_found() -> Response {
    with_cache(
        StatusCode::NOT_FOUND,
        NO_STORE,
        json!({ "error": "Not found" }),
    )
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

async fn every_response(mut res: Response) -> Response {
    let h = res.headers_mut();
    h.insert("x-mzizi-source", HeaderValue::from_static("registry"));
    h.insert("access-control-allow-origin", HeaderValue::from_static("*"));
    h.insert(
        "access-control-allow-methods",
        HeaderValue::from_static("GET, HEAD, OPTIONS"),
    );
    h.insert(
        "access-control-allow-headers",
        HeaderValue::from_static("content-type"),
    );
    res
}

fn app() -> Router {
    Router::new()
        .route("/v1/health", get(health).fallback(other_method))
        .route("/v1/ui", get(list).fallback(other_method))
        .route("/v1/ui/{name}", get(item).fallback(other_method))
        .route("/v1/components", get(gone).fallback(other_method))
        .route("/v1/themes", get(gone).fallback(other_method))
        .route("/v1/versions", get(versions).fallback(other_method))
        .fallback(fallback)
        .layer(middleware::map_response(every_response))
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
