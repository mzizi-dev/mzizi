//! B4's Rust reference (benchmarks/tasks/b4-state/spec.md), in plain axum.

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::any;
use axum::{Json, Router, middleware};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

const ALLOW: &str = "GET, HEAD, POST, OPTIONS";

type History = Arc<Mutex<HashMap<String, Vec<String>>>>;

fn problem(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

fn not_found() -> Response {
    problem(StatusCode::NOT_FOUND, "Not found")
}

/// `{"version": "<non-empty string>"}`, else nothing.
fn version_of(body: &[u8]) -> Option<String> {
    let value: Value = serde_json::from_slice(body).ok()?;
    let version = value.as_object()?.get("version")?.as_str()?;
    (!version.is_empty()).then(|| version.to_string())
}

async fn versions(
    State(history): State<History>,
    Path(name): Path<String>,
    method: Method,
    body: Bytes,
) -> Response {
    if name != "badge" && name != "button" {
        return not_found();
    }
    // A poisoned lock means another request panicked; serve the data anyway.
    let mut history = history.lock().unwrap_or_else(|e| e.into_inner());
    let list = history.entry(name.clone()).or_default();
    match method {
        Method::GET | Method::HEAD => {
            Json(json!({ "component": name, "versions": list, "count": list.len() }))
                .into_response()
        }
        Method::POST => {
            let Some(version) = version_of(&body) else {
                return problem(StatusCode::BAD_REQUEST, "Invalid body");
            };
            if list.contains(&version) {
                return problem(StatusCode::CONFLICT, "Version already recorded");
            }
            list.push(version.clone());
            let count = list.len();
            (
                StatusCode::CREATED,
                [(header::LOCATION, format!("/v1/ui/{name}/versions"))],
                Json(json!({ "component": name, "version": version, "count": count })),
            )
                .into_response()
        }
        Method::OPTIONS => (StatusCode::NO_CONTENT, [(header::ALLOW, ALLOW)]).into_response(),
        _ => (StatusCode::METHOD_NOT_ALLOWED, [(header::ALLOW, ALLOW)]).into_response(),
    }
}

async fn source(mut res: Response) -> Response {
    res.headers_mut()
        .insert("x-mzizi-source", HeaderValue::from_static("registry"));
    res
}

fn app() -> Router {
    Router::new()
        .route("/v1/ui/{name}/versions", any(versions))
        .fallback(|| async { not_found() })
        .with_state(History::default())
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
