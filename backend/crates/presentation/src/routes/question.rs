use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use omypic_domain::question::QuestionGroupAggregate;
use serde::{Deserialize, Serialize};

use super::{iso, strings};
use crate::error::{blocking, ApiError, ApiJson};
use crate::state::AppState;

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/question-groups", get(list).post(create))
        .route("/api/question-groups/tags/stats", get(tag_stats))
        .route(
            "/api/question-groups/{group_id}",
            put(update_group).delete(delete_group),
        )
        .route("/api/question-groups/{group_id}/items", post(add_question))
        .route(
            "/api/question-groups/{group_id}/items/{item_id}",
            put(update_question).delete(delete_question),
        )
}

#[derive(Deserialize)]
struct QuestionGroupRequest {
    name: Option<String>,
    tags: Option<Vec<Option<String>>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct QuestionItemRequest {
    text: Option<String>,
    question_type: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct QuestionGroupResponse {
    id: String,
    name: String,
    tags: Vec<String>,
    questions: Vec<QuestionItemResponse>,
    created_at: String,
    updated_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct QuestionItemResponse {
    id: String,
    text: String,
    question_type: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct QuestionTagStatResponse {
    tag: String,
    group_count: u64,
    selectable: bool,
}

fn to_response(group: &QuestionGroupAggregate) -> QuestionGroupResponse {
    // Java `sorted()`는 UTF-16 코드 유닛 순서다.
    let mut tags = group.tags().to_vec();
    tags.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
    QuestionGroupResponse {
        id: group.id().value().to_string(),
        name: group.name().to_string(),
        tags,
        questions: group
            .questions()
            .iter()
            .map(|item| QuestionItemResponse {
                id: item.id().value().to_string(),
                text: item.text().to_string(),
                question_type: item.question_type().map(str::to_string),
            })
            .collect(),
        created_at: iso(group.created_at()),
        updated_at: iso(group.updated_at()),
    }
}

async fn list(State(state): State<AppState>) -> Result<Json<Vec<QuestionGroupResponse>>, ApiError> {
    let use_case = state.question.clone();
    let groups = blocking(move || use_case.list()).await?;
    Ok(Json(groups.iter().map(to_response).collect()))
}

async fn create(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<QuestionGroupRequest>,
) -> Result<Json<QuestionGroupResponse>, ApiError> {
    let use_case = state.question.clone();
    let name = body.name.unwrap_or_default();
    let tags = strings(body.tags);
    let group = blocking(move || use_case.create_group(&name, &tags)).await?;
    Ok(Json(to_response(&group)))
}

async fn update_group(
    State(state): State<AppState>,
    Path(group_id): Path<String>,
    ApiJson(body): ApiJson<QuestionGroupRequest>,
) -> Result<Json<QuestionGroupResponse>, ApiError> {
    let use_case = state.question.clone();
    let name = body.name.unwrap_or_default();
    let tags = strings(body.tags);
    let group = blocking(move || use_case.update_group(&group_id, &name, &tags)).await?;
    Ok(Json(to_response(&group)))
}

async fn delete_group(
    State(state): State<AppState>,
    Path(group_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let use_case = state.question.clone();
    blocking(move || use_case.delete_group(&group_id)).await?;
    Ok(StatusCode::OK)
}

async fn add_question(
    State(state): State<AppState>,
    Path(group_id): Path<String>,
    ApiJson(item): ApiJson<QuestionItemRequest>,
) -> Result<Json<QuestionGroupResponse>, ApiError> {
    let use_case = state.question.clone();
    let text = item.text.unwrap_or_default();
    let group =
        blocking(move || use_case.add_question(&group_id, &text, item.question_type.as_deref()))
            .await?;
    Ok(Json(to_response(&group)))
}

async fn update_question(
    State(state): State<AppState>,
    Path((group_id, item_id)): Path<(String, String)>,
    ApiJson(item): ApiJson<QuestionItemRequest>,
) -> Result<Json<QuestionGroupResponse>, ApiError> {
    let use_case = state.question.clone();
    let text = item.text.unwrap_or_default();
    let group = blocking(move || {
        use_case.update_question(&group_id, &item_id, &text, item.question_type.as_deref())
    })
    .await?;
    Ok(Json(to_response(&group)))
}

async fn delete_question(
    State(state): State<AppState>,
    Path((group_id, item_id)): Path<(String, String)>,
) -> Result<Json<QuestionGroupResponse>, ApiError> {
    let use_case = state.question.clone();
    let group = blocking(move || use_case.delete_question(&group_id, &item_id)).await?;
    Ok(Json(to_response(&group)))
}

async fn tag_stats(
    State(state): State<AppState>,
) -> Result<Json<Vec<QuestionTagStatResponse>>, ApiError> {
    let use_case = state.question.clone();
    let stats = blocking(move || use_case.list_tag_stats()).await?;
    Ok(Json(
        stats
            .into_iter()
            .map(|stat| QuestionTagStatResponse {
                tag: stat.tag,
                group_count: stat.group_count,
                selectable: stat.selectable,
            })
            .collect(),
    ))
}
