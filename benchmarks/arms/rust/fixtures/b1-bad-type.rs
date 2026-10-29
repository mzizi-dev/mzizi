//! Known-bad, for the check: a type error. `check.sh` must exit 1.
use axum::{Router, routing::get};

#[tokio::main]
async fn main() {
    let count: u32 = "2";
    let app = Router::new().route("/v1/ui", get(|| async move { count.to_string() }));
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
