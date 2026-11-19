use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::{json, Value};

use crate::error::ApiError;
use crate::state::AppState;

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/actuator/health", get(health))
        .route("/actuator/info", get(info))
        .route("/actuator/metrics", get(metric_names))
        .route("/actuator/metrics/{name}", get(metric))
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "UP" }))
}

async fn info() -> Json<Value> {
    Json(json!({}))
}

async fn metric_names(State(state): State<AppState>) -> Json<Value> {
    Json(json!({ "names": state.metrics.names() }))
}

async fn metric(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let snapshot = state.metrics.snapshot(&name).ok_or_else(|| {
        ApiError::Status(
            axum::http::StatusCode::NOT_FOUND,
            format!("No metric named '{name}'"),
        )
    })?;
    let measurements: Vec<Value> = snapshot
        .measurements
        .iter()
        .map(|(statistic, value)| json!({ "statistic": statistic, "value": value }))
        .collect();
    Ok(Json(
        json!({ "name": snapshot.name, "measurements": measurements }),
    ))
}
