use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use omypic_application::apikey::model::ApiKeyVerifyCommand;
use serde::{Deserialize, Serialize};

use super::iso;
use crate::error::{blocking, require_not_blank, ApiError, ApiJson};
use crate::state::AppState;

pub(super) fn routes() -> Router<AppState> {
    Router::new().route("/api/keys/verify", post(verify))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiKeyVerifyRequest {
    provider: Option<String>,
    api_key: Option<String>,
    model: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ApiKeyVerifyResponse {
    valid: bool,
    provider: String,
    checked_at: String,
    message: String,
}

async fn verify(
    State(state): State<AppState>,
    ApiJson(request): ApiJson<ApiKeyVerifyRequest>,
) -> Result<Json<ApiKeyVerifyResponse>, ApiError> {
    require_not_blank("provider", &request.provider)?;
    require_not_blank("apiKey", &request.api_key)?;
    let command = ApiKeyVerifyCommand {
        provider: request.provider,
        api_key: request.api_key,
        model: request.model,
    };
    let use_case = state.api_key_verify.clone();
    let result = blocking(move || use_case.verify(&command)).await?;
    Ok(Json(ApiKeyVerifyResponse {
        valid: result.valid,
        provider: result.provider,
        checked_at: iso(result.checked_at),
        message: result.message,
    }))
}
