pub use crate::query_metric::query_metric_handler;
pub use crate::query_metric_tags::query_metric_tags_handler;

use axum::{response::IntoResponse, Json};
use serde_json::json;

pub async fn health_handler() -> impl IntoResponse {
    // Simple readiness/health endpoint. Keep it lightweight.
    Json(json!({ "status": "ok" }))
}
