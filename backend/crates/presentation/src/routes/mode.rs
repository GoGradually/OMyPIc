use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::put;
use axum::Router;
use omypic_application::session::model::ModeUpdateCommand;
use omypic_domain::session::ModeType;
use serde::Deserialize;

use super::strings;
use crate::error::{blocking, ApiError, ApiJson};
use crate::state::AppState;

pub(super) fn routes() -> Router<AppState> {
    Router::new().route("/api/modes", put(update))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModeUpdateRequest {
    session_id: Option<String>,
    mode: Option<String>,
    continuous_batch_size: Option<i32>,
    selected_group_tags: Option<Vec<Option<String>>>,
}

/// Jackson 열거형 변환처럼 이름이 정확히 같아야 한다.
fn parse_mode(mode: Option<String>) -> Result<Option<ModeType>, ApiError> {
    match mode.as_deref() {
        None => Ok(None),
        Some("IMMEDIATE") => Ok(Some(ModeType::Immediate)),
        Some("CONTINUOUS") => Ok(Some(ModeType::Continuous)),
        Some(_) => Err(ApiError::BadRequest("Unsupported mode".to_string())),
    }
}

async fn update(
    State(state): State<AppState>,
    ApiJson(request): ApiJson<ModeUpdateRequest>,
) -> Result<StatusCode, ApiError> {
    let command = ModeUpdateCommand {
        session_id: request.session_id.unwrap_or_default(),
        mode: parse_mode(request.mode)?,
        continuous_batch_size: request.continuous_batch_size,
        selected_group_tags: strings(request.selected_group_tags),
    };
    let use_case = state.session.clone();
    blocking(move || use_case.update_mode(&command).map(|_| ())).await?;
    Ok(StatusCode::NO_CONTENT)
}
