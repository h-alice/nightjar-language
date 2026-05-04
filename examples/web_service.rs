//! Web service example — Axum handler that maps `ExecResult` to HTTP.
//!
//! Accepts `POST /validate` with body `{"rule": "...", "data": {...}}` and
//! responds with `200 True`, `400 False`, or `400 Error + diagnostic`.
//!
//! The interesting part is the `ExecResult` -> HTTP status mapping; the
//! Axum scaffolding is just the smallest container that exercises it.
//!
//! Run: `cargo run --example web_service`
//!
//! Test:
//! ```sh
//! curl -sS http://127.0.0.1:3000/validate \
//!   -H 'content-type: application/json' \
//!   -d '{"rule":"(GE .revenue 0)","data":{"revenue":42}}'
//! ```

use axum::{extract::Json, http::StatusCode, routing::post, Router};
use nightjar_lang::{exec, ExecOptions, ExecResult};
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
struct VerifyRequest {
    rule: String,
    data: serde_json::Value,
}

async fn verify(Json(req): Json<VerifyRequest>) -> (StatusCode, Json<serde_json::Value>) {
    match exec(&req.rule, req.data, ExecOptions::default()) {
        ExecResult::True => (StatusCode::OK, Json(json!({"result": "True"}))),
        ExecResult::False => (StatusCode::BAD_REQUEST, Json(json!({"result": "False"}))),
        ExecResult::Error(e) => (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "result":  "Error",
                "code":    format!("{:?}", e.code()),
                "span":    { "start": e.span().start, "end": e.span().end },
                "message": e.message(),
            })),
        ),
    }
}

#[tokio::main]
async fn main() {
    let app = Router::new().route("/validate", post(verify));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();
    println!("listening on http://127.0.0.1:3000");
    axum::serve(listener, app).await.unwrap();
}
