use std::collections::HashMap;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, put};
use axum::{Json, Router};
use omypic_domain::rulebook::{Rulebook, RulebookScope};
use serde::Serialize;

use super::iso;
use crate::error::{blocking, multipart_error, ApiError, ApiMultipart};
use crate::state::AppState;

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/rulebooks", get(list).post(upload))
        .route("/api/rulebooks/{id}/toggle", put(toggle))
        .route("/api/rulebooks/{id}", axum::routing::delete(delete))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RulebookResponse {
    id: String,
    filename: String,
    path: String,
    scope: &'static str,
    question_group: Option<String>,
    enabled: bool,
    created_at: String,
    updated_at: String,
}

fn to_response(rulebook: &Rulebook) -> RulebookResponse {
    RulebookResponse {
        id: rulebook.id().value().to_string(),
        filename: rulebook.filename().to_string(),
        path: rulebook.path().to_string(),
        scope: match rulebook.scope() {
            RulebookScope::Main => "MAIN",
            RulebookScope::Question => "QUESTION",
        },
        question_group: rulebook
            .question_group()
            .map(|group| group.value().to_string()),
        enabled: rulebook.is_enabled(),
        created_at: iso(rulebook.created_at()),
        updated_at: iso(rulebook.updated_at()),
    }
}

/// Spring 열거형 파라미터 변환처럼 앞뒤 공백을 지우고 이름이 정확히 같아야 한다.
/// 빈 값은 없는 것으로 본다.
fn parse_scope(raw: Option<String>) -> Result<Option<RulebookScope>, ApiError> {
    match raw.as_deref().map(str::trim) {
        None | Some("") => Ok(None),
        Some("MAIN") => Ok(Some(RulebookScope::Main)),
        Some("QUESTION") => Ok(Some(RulebookScope::Question)),
        Some(_) => Err(ApiError::BadRequest("Invalid scope".to_string())),
    }
}

/// Spring `StringToBooleanConverter`와 같다.
fn parse_bool(raw: Option<&String>) -> Result<bool, ApiError> {
    let missing =
        || ApiError::BadRequest("Required parameter 'enabled' is not present.".to_string());
    let value = raw.map(|v| v.trim().to_lowercase()).ok_or_else(missing)?;
    match value.as_str() {
        "" => Err(missing()),
        "true" | "on" | "yes" | "1" => Ok(true),
        "false" | "off" | "no" | "0" => Ok(false),
        _ => Err(ApiError::BadRequest(
            "Invalid value for 'enabled'".to_string(),
        )),
    }
}

async fn list(State(state): State<AppState>) -> Result<Json<Vec<RulebookResponse>>, ApiError> {
    let use_case = state.rulebook.clone();
    let rulebooks = blocking(move || use_case.list()).await?;
    Ok(Json(rulebooks.iter().map(to_response).collect()))
}

async fn upload(
    State(state): State<AppState>,
    ApiMultipart(mut multipart): ApiMultipart,
) -> Result<Json<RulebookResponse>, ApiError> {
    let mut file: Option<(Option<String>, Vec<u8>)> = None;
    let mut scope: Option<String> = None;
    let mut question_group: Option<String> = None;
    while let Some(field) = multipart.next_field().await.map_err(multipart_error)? {
        match field.name() {
            Some("file") => {
                let filename = field.file_name().map(str::to_string);
                let bytes = field.bytes().await.map_err(multipart_error)?.to_vec();
                file = Some((filename, bytes));
            }
            Some("scope") => scope = Some(field.text().await.map_err(multipart_error)?),
            Some("questionGroup") => {
                question_group = Some(field.text().await.map_err(multipart_error)?)
            }
            _ => {}
        }
    }
    let (filename, bytes) = file
        .ok_or_else(|| ApiError::BadRequest("Required part 'file' is not present.".to_string()))?;
    let scope = parse_scope(scope)?;
    let use_case = state.rulebook.clone();
    let rulebook = blocking(move || {
        use_case.upload(
            filename.as_deref(),
            &bytes,
            scope,
            question_group.as_deref(),
        )
    })
    .await?;
    Ok(Json(to_response(&rulebook)))
}

async fn toggle(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<RulebookResponse>, ApiError> {
    let enabled = parse_bool(params.get("enabled"))?;
    let use_case = state.rulebook.clone();
    let rulebook = blocking(move || use_case.toggle(&id, enabled)).await?;
    Ok(Json(to_response(&rulebook)))
}

async fn delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let use_case = state.rulebook.clone();
    blocking(move || use_case.delete(&id)).await?;
    Ok(StatusCode::OK)
}
