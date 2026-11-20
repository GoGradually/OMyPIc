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

/// `GET /api/data-transfer/export`: 백업 zip을 첨부 파일로 돌려준다(200, `application/zip`).
///
/// 성공 응답은 이것 하나뿐이다. 행은 있지만 파일이 데이터 폴더에서 사라진 룰북은 백업에서 빼고
/// 내보내기는 성공한다. 그런 룰북이 하나 이상이면 응답에 `x-omypic-missing-rulebook-files:
/// <뺀 룰북 수>`(10진 정수, 1 이상)가 붙고, 없으면 이 헤더가 아예 없다(없음 = 0). 브라우저에서
/// 읽을 수 있게 CORS `Access-Control-Expose-Headers`에 들어 있다. 백업에 든 다른 룰북과 데이터는
/// 그대로이고, 뺀 룰북의 id와 파일 이름은 zip의 `manifest.json` `missingRulebookFiles`에 있다
/// (가져오기는 이 목록을 무시한다). 실패하면 이 헤더 없이 오류 JSON이다.
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
