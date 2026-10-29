//! B5's Rust reference (benchmarks/tasks/b5-errors/spec.md), in plain axum.

use axum::body::{Body, to_bytes};
use axum::extract::Request;
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router, middleware};
use serde_json::{Value, json};

const MAX_BODY: usize = 1024;

fn problem(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

fn not_found() -> Response {
    problem(StatusCode::NOT_FOUND, "Not found")
}

/// An internal failure. The cause goes to the log; the response never mentions it.
struct Internal(String);

impl IntoResponse for Internal {
    fn into_response(self) -> Response {
        eprintln!("internal error: {}", self.0);
        problem(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error")
    }
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

async fn fail() -> Result<Json<Value>, Internal> {
    Err(Internal("the work behind /v1/fail broke".to_string()))
}

fn is_json(headers: &HeaderMap) -> bool {
    headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(';').next())
        .is_some_and(|m| m.trim().eq_ignore_ascii_case("application/json"))
}

async fn messages(headers: HeaderMap, body: Body) -> Response {
    if !is_json(&headers) {
        return problem(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Expected application/json",
        );
    }
    let Ok(bytes) = to_bytes(body, MAX_BODY).await else {
        return problem(StatusCode::PAYLOAD_TOO_LARGE, "Payload too large");
    };
    let Ok(value) = serde_json::from_slice::<Value>(&bytes) else {
        return problem(StatusCode::BAD_REQUEST, "Malformed JSON");
    };
    match value.get("text").and_then(Value::as_str) {
        Some(text) if !text.is_empty() => {
            (StatusCode::CREATED, Json(json!({ "text": text }))).into_response()
        }
        _ => problem(StatusCode::BAD_REQUEST, "Invalid message"),
    }
}

/// OPTIONS, and every method a route does not declare.
async fn get_route_methods(method: Method) -> Response {
    other_methods(method, "GET, HEAD, OPTIONS")
}

async fn post_route_methods(method: Method) -> Response {
    other_methods(method, "POST, OPTIONS")
}

fn other_methods(method: Method, allow: &'static str) -> Response {
    let status = if method == Method::OPTIONS {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::METHOD_NOT_ALLOWED
    };
    (status, [(header::ALLOW, allow)]).into_response()
}

/// A path no route matches: the canonical-path redirect, else the not-found response.
async fn fallback(req: Request) -> Response {
    let path = req.uri().path();
    let mut canonical = String::from("/");
    canonical.push_str(
        &path
            .split('/')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("/"),
    );
    if canonical != path {
        if let Some(q) = req.uri().query() {
            canonical.push('?');
            canonical.push_str(q);
        }
        return (
            StatusCode::PERMANENT_REDIRECT,
            [(header::LOCATION, canonical)],
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
        .route("/v1/health", get(health).fallback(get_route_methods))
        .route("/v1/fail", get(fail).fallback(get_route_methods))
        .route("/v1/messages", post(messages).fallback(post_route_methods))
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
