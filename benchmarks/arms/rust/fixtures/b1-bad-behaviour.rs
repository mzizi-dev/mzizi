//! Known-bad, for the probes: compiles clean, but serves only the three GET routes with axum's
//! defaults. It has no x-mzizi-source header, no OPTIONS handling, an empty 404 and a bare
//! 405, and no trailing-slash redirect, so most of B1's probes must fail on it.
use axum::extract::Path;
use axum::routing::get;
use axum::{Json, Router};
use serde_json::json;

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/v1/health", get(|| async { Json(json!({ "status": "ok" })) }))
        .route("/v1/ui", get(|| async { Json(json!({ "count": 2 })) }))
        .route(
            "/v1/ui/{name}",
            get(|Path(name): Path<String>| async move { Json(json!({ "name": name })) }),
        );
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
