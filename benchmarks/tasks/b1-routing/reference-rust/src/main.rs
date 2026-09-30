//! B1's Rust reference (benchmarks/tasks/b1-routing/spec.md), in plain axum.

use axum::extract::{Path, Request};
use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router, middleware};
use serde_json::json;

const ALLOW: &str = "GET, HEAD, OPTIONS";
const NO_STORE: &str = "private, no-cache, no-store, max-age=0, must-revalidate";

async fn health() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok" }))
}

async fn list() -> Json<serde_json::Value> {
    Json(json!({ "count": 2 }))
}

async fn item(Path(name): Path<String>) -> Response {
    match name.as_str() {
        "badge" | "button" => Json(json!({ "name": name })).into_response(),
        _ => not_found(),
    }
}

fn not_found() -> Response {
    (
        StatusCode::NOT_FOUND,
        [(header::CACHE_CONTROL, NO_STORE)],
        Json(json!({ "error": "Not found" })),
    )
        .into_response()
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
        if location.is_empty() {
            location.push('/');
        }
        if let Some(q) = req.uri().query() {
            location.push('?');
            location.push_str(q);
        }
        return (StatusCode::PERMANENT_REDIRECT, [(header::LOCATION, location)]).into_response();
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
        .route("/v1/health", get(health).fallback(other_method))
        .route("/v1/ui", get(list).fallback(other_method))
        .route("/v1/ui/{name}", get(item).fallback(other_method))
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
