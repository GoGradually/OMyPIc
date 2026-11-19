use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;

use super::iso;
use crate::error::{blocking, ApiError};
use crate::state::AppState;

pub(super) fn routes() -> Router<AppState> {
    Router::new().route("/api/wrongnotes", get(list))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WrongNoteResponse {
    id: String,
    pattern: String,
    count: i32,
    short_summary: String,
    last_seen_at: String,
}

async fn list(State(state): State<AppState>) -> Result<Json<Vec<WrongNoteResponse>>, ApiError> {
    let use_case = state.wrong_note.clone();
    let notes = blocking(move || use_case.list()).await?;
    Ok(Json(
        notes
            .iter()
            .map(|note| WrongNoteResponse {
                id: note.id().value().to_string(),
                pattern: note.pattern().to_string(),
                count: note.count(),
                short_summary: note.short_summary().to_string(),
                last_seen_at: iso(note.last_seen_at()),
            })
            .collect(),
    ))
}
