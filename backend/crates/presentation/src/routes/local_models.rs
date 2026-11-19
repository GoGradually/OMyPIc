//! 온디바이스 음성 모델 파일(`@huggingface/transformers`와 `kokoro-js`가 받는 파일)을 백엔드가
//! 받아 보관했다가 내보낸다. 경로는 Hugging Face의 `resolve` 주소와 같은 모양이다.

use std::io::SeekFrom;

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use omypic_application::shared::local_model::LocalModelError;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;

use crate::error::{blocking, ApiError};
use crate::state::AppState;

const STREAM_CHUNK_BYTES: usize = 64 * 1024;

pub(super) fn routes() -> Router<AppState> {
    // axum은 GET 경로에 오는 HEAD 요청을 본문만 뺀 같은 응답으로 처리한다.
    Router::new().route(
        "/api/local-models/{org}/{name}/resolve/{revision}/{*file}",
        get(serve),
    )
}

async fn serve(
    State(state): State<AppState>,
    Path((org, name, revision, file)): Path<(String, String, String, String)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let files = state.local_models.clone();
    let repo = format!("{org}/{name}");
    let cached = blocking(move || Ok(files.fetch(&repo, &revision, &file)))
        .await?
        .map_err(model_error)?;

    let io_error = |err: std::io::Error| {
        tracing::error!(error = %err, path = %cached.path.display(), "failed to read cached model file");
        ApiError::Internal
    };
    let mut handle = tokio::fs::File::open(&cached.path)
        .await
        .map_err(io_error)?;
    let total = handle.metadata().await.map_err(io_error)?.len();
    let content_type =
        HeaderValue::from_str(&cached.content_type).map_err(|_| ApiError::Internal)?;

    let range = match headers
        .get(header::RANGE)
        .and_then(|value| value.to_str().ok())
    {
        Some(value) => parse_range(value, total),
        None => RangeRequest::Full,
    };
    let (status, start, length, content_range) = match range {
        RangeRequest::Full => (StatusCode::OK, 0, total, None),
        RangeRequest::Partial { start, end } => (
            StatusCode::PARTIAL_CONTENT,
            start,
            end - start + 1,
            Some(format!("bytes {start}-{end}/{total}")),
        ),
        RangeRequest::Unsatisfiable => {
            return Ok((
                StatusCode::RANGE_NOT_SATISFIABLE,
                [(header::CONTENT_RANGE, format!("bytes */{total}"))],
            )
                .into_response());
        }
    };
    if start > 0 {
        handle
            .seek(SeekFrom::Start(start))
            .await
            .map_err(io_error)?;
    }

    let body = Body::from_stream(ReaderStream::with_capacity(
        handle.take(length),
        STREAM_CHUNK_BYTES,
    ));
    let mut response = (status, body).into_response();
    let response_headers = response.headers_mut();
    response_headers.insert(header::CONTENT_TYPE, content_type);
    response_headers.insert(header::CONTENT_LENGTH, HeaderValue::from(length));
    response_headers.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    if let Some(value) = content_range.and_then(|value| HeaderValue::from_str(&value).ok()) {
        response_headers.insert(header::CONTENT_RANGE, value);
    }
    Ok(response)
}

fn model_error(error: LocalModelError) -> ApiError {
    match error {
        LocalModelError::InvalidPath(message) => ApiError::BadRequest(message),
        LocalModelError::NotAllowed(message) => ApiError::Status(StatusCode::FORBIDDEN, message),
        LocalModelError::NotFound(message) => ApiError::Status(StatusCode::NOT_FOUND, message),
        LocalModelError::Upstream(message) => {
            tracing::warn!(error = %message, "local model upstream failed");
            ApiError::Status(StatusCode::BAD_GATEWAY, message)
        }
        LocalModelError::Internal(message) => {
            tracing::error!(error = %message, "local model cache failed");
            ApiError::Internal
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum RangeRequest {
    Full,
    Partial { start: u64, end: u64 },
    Unsatisfiable,
}

/// `bytes=a-b`, `bytes=a-`, `bytes=-n` 하나만 받는다. 그 밖의 값(여러 구간, 다른 단위, 잘못된 값)은
/// 헤더가 없는 것처럼 전체를 보낸다(RFC 9110이 허용한다).
fn parse_range(value: &str, total: u64) -> RangeRequest {
    let Some(spec) = value.trim().strip_prefix("bytes=") else {
        return RangeRequest::Full;
    };
    let Some((first, last)) = spec.split_once('-') else {
        return RangeRequest::Full;
    };
    let (first, last) = (first.trim(), last.trim());
    let (start, end) = match (first.parse::<u64>(), last.parse::<u64>()) {
        (Ok(start), Ok(end)) if start <= end => (start, end),
        (Ok(start), Err(_)) if last.is_empty() => (start, u64::MAX),
        (Err(_), Ok(suffix)) if first.is_empty() && suffix > 0 => {
            (total.saturating_sub(suffix), u64::MAX)
        }
        _ => return RangeRequest::Full,
    };
    if start >= total {
        return RangeRequest::Unsatisfiable;
    }
    RangeRequest::Partial {
        start,
        end: end.min(total - 1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_single_byte_ranges() {
        assert_eq!(
            RangeRequest::Partial { start: 0, end: 0 },
            parse_range("bytes=0-0", 10)
        );
        assert_eq!(
            RangeRequest::Partial { start: 2, end: 9 },
            parse_range("bytes=2-", 10)
        );
        assert_eq!(
            RangeRequest::Partial { start: 2, end: 4 },
            parse_range("bytes=2-4", 10)
        );
        assert_eq!(
            RangeRequest::Partial { start: 2, end: 9 },
            parse_range("bytes=2-99", 10)
        );
        assert_eq!(
            RangeRequest::Partial { start: 7, end: 9 },
            parse_range("bytes=-3", 10)
        );
        assert_eq!(
            RangeRequest::Partial { start: 0, end: 9 },
            parse_range("bytes=-30", 10)
        );
    }

    #[test]
    fn unsupported_ranges_serve_the_whole_file_and_out_of_range_is_416() {
        for value in [
            "items=0-1",
            "bytes=0-1,3-4",
            "bytes=a-b",
            "bytes=4-2",
            "bytes=-",
            "bytes=-0",
            "0-1",
        ] {
            assert_eq!(RangeRequest::Full, parse_range(value, 10), "{value}");
        }
        assert_eq!(RangeRequest::Unsatisfiable, parse_range("bytes=10-", 10));
        assert_eq!(RangeRequest::Unsatisfiable, parse_range("bytes=0-0", 0));
    }
}
