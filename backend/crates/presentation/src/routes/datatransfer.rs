use axum::extract::State;
use axum::http::{header, HeaderName, HeaderValue};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use serde::Serialize;

use super::iso;
use crate::error::{blocking, multipart_error, ApiError, ApiMultipart};
use crate::state::AppState;

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/data-transfer/export", get(export_data))
        .route("/api/data-transfer/import", post(import_data))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DataTransferImportResponse {
    imported_at: String,
    question_group_count: usize,
    rulebook_count: usize,
    wrong_note_count: usize,
    wrong_note_queue_size: usize,
    restart_required: bool,
}

/// 파일이 없어 백업에서 뺀 룰북 수. 하나도 없으면 보내지 않는다. 어떤 룰북인지는 백업의
/// `manifest.json`(`missingRulebookFiles`)과 서버 로그에 있다.
pub const MISSING_RULEBOOK_FILES_HEADER: &str = "x-omypic-missing-rulebook-files";

async fn export_data(State(state): State<AppState>) -> Result<Response, ApiError> {
    let use_case = state.data_transfer.clone();
    let export = blocking(move || use_case.export_backup()).await?;
    let missing = export.missing_rulebook_files.len();
    let bytes = export.zip;
    let filename = format!("omypic-backup-{}.zip", Utc::now().format("%Y%m%d-%H%M%S"));
    let disposition = HeaderValue::from_str(&format!("attachment; filename=\"{filename}\""))
        .map_err(|_| ApiError::Internal)?;
    let mut response = (
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/zip"),
            ),
            (header::CONTENT_DISPOSITION, disposition),
        ],
        bytes,
    )
        .into_response();
    if missing > 0 {
        response.headers_mut().insert(
            HeaderName::from_static(MISSING_RULEBOOK_FILES_HEADER),
            HeaderValue::from(missing),
        );
    }
    Ok(response)
}

async fn import_data(
    State(state): State<AppState>,
    ApiMultipart(mut multipart): ApiMultipart,
) -> Result<Json<DataTransferImportResponse>, ApiError> {
    let mut file: Option<Vec<u8>> = None;
    while let Some(field) = multipart.next_field().await.map_err(multipart_error)? {
        if field.name() == Some("file") {
            file = Some(field.bytes().await.map_err(multipart_error)?.to_vec());
        }
    }
    let bytes = file
        .ok_or_else(|| ApiError::BadRequest("Required part 'file' is not present.".to_string()))?;
    let use_case = state.data_transfer.clone();
    let result = blocking(move || use_case.import_zip(&bytes)).await?;
    Ok(Json(DataTransferImportResponse {
        imported_at: iso(result.imported_at),
        question_group_count: result.question_group_count,
        rulebook_count: result.rulebook_count,
        wrong_note_count: result.wrong_note_count,
        wrong_note_queue_size: result.wrong_note_queue_size,
        restart_required: result.restart_required,
    }))
}
