//! 온디바이스 음성 모델 파일(`@huggingface/transformers`와 `kokoro-js`가 받는 파일)을 백엔드가
//! 받아 보관했다가 내보낸다. 경로는 Hugging Face의 `resolve` 주소와 같은 모양이다.
//!
//! 처음 받는 파일은 다 받기를 기다리지 않는다. 상위 서버의 응답 헤더가 오면 바로 답하고, 받는
//! 대로 본문을 보낸다. 받기가 도중에 실패하면 응답을 짧게 끊어 클라이언트가 본문이 모자란 것을
//! 알게 한다(`Content-Length`가 있으면 연결을 끊는다). 캐시에 없는 파일의 정보 요청(HEAD,
//! `Range: bytes=0-0`)은 상위 서버에 같은 것을 물어 답하고 받지 않는다.

use std::io::{Read, SeekFrom};
use std::pin::Pin;
use std::task::{Context, Poll};

use axum::body::{Body, Bytes};
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use omypic_application::shared::local_model::{
    CachedModelFile, DownloadingModelFile, LocalModelError, ModelFile, ModelFileProbe,
    UpstreamModelReply,
};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio::sync::mpsc;
use tokio_util::io::ReaderStream;

use crate::error::{blocking, ApiError};
use crate::state::AppState;

const STREAM_CHUNK_BYTES: usize = 64 * 1024;
/// 블로킹 스레드가 읽어 두는 조각 수의 한도. 클라이언트가 느리면 이 스레드의 읽기만 멈추고
/// (메모리에 쌓지 않는다) 상위 서버에서 받는 일은 계속된다.
const STREAM_CHANNEL_CHUNKS: usize = 4;

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
    method: Method,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let files = state.local_models.clone();
    let repo = format!("{org}/{name}");
    let head = method == Method::HEAD;
    let range = headers
        .get(header::RANGE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let probe = (head || range.is_some()).then(|| ModelFileProbe {
        head,
        range: range.clone(),
    });
    let opened = blocking(move || Ok(files.open(&repo, &revision, &file, probe.as_ref())))
        .await?
        .map_err(model_error)?;
    match opened {
        ModelFile::Cached(cached) => serve_cached(cached, range.as_deref()).await,
        ModelFile::Downloading(download) => serve_download(download, range.as_deref(), head),
        ModelFile::Upstream(reply) => relay(reply, head),
    }
}

/// 받는 중인 파일을 받는 대로 보낸다. 전체 크기를 알면 `Range`도 그 안에서 답한다.
fn serve_download(
    download: DownloadingModelFile,
    range: Option<&str>,
    head: bool,
) -> Result<Response, ApiError> {
    let (status, start, length, content_range) = match (range, download.length) {
        (Some(value), Some(total)) => match parse_range(value, total) {
            RangeRequest::Full => (StatusCode::OK, 0, Some(total), None),
            RangeRequest::Partial { start, end } => (
                StatusCode::PARTIAL_CONTENT,
                start,
                Some(end - start + 1),
                Some(format!("bytes {start}-{end}/{total}")),
            ),
            RangeRequest::Unsatisfiable => return Ok(unsatisfiable(total)),
        },
        // 전체 크기를 모르면 `Range`를 무시하고 전체를 보낸다(RFC 9110이 허용한다).
        (_, total) => (StatusCode::OK, 0, total, None),
    };
    let body = if head {
        Body::empty()
    } else {
        stream_blocking(download.body, start, length)
    };
    with_headers(
        (status, body).into_response(),
        &download.content_type,
        length,
        content_range,
    )
}

/// 캐시에 없는 파일의 정보 요청에 상위 서버가 한 답을 옮긴다.
fn relay(reply: UpstreamModelReply, head: bool) -> Result<Response, ApiError> {
    let status = StatusCode::from_u16(reply.status).map_err(|_| ApiError::Internal)?;
    let body = if head {
        Body::empty()
    } else {
        stream_blocking(reply.body, 0, reply.content_length)
    };
    with_headers(
        (status, body).into_response(),
        &reply.content_type,
        reply.content_length,
        reply.content_range,
    )
}

fn with_headers(
    mut response: Response,
    content_type: &str,
    length: Option<u64>,
    content_range: Option<String>,
) -> Result<Response, ApiError> {
    let content_type = HeaderValue::from_str(content_type).map_err(|_| ApiError::Internal)?;
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, content_type);
    if let Some(length) = length {
        headers.insert(header::CONTENT_LENGTH, HeaderValue::from(length));
    }
    headers.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    if let Some(value) = content_range.and_then(|value| HeaderValue::from_str(&value).ok()) {
        headers.insert(header::CONTENT_RANGE, value);
    }
    Ok(response)
}

fn unsatisfiable(total: u64) -> Response {
    (
        StatusCode::RANGE_NOT_SATISFIABLE,
        [(header::CONTENT_RANGE, format!("bytes */{total}"))],
    )
        .into_response()
}

/// 블로킹 `reader`를 블로킹 스레드에서 읽어 본문으로 보낸다. 앞의 `skip` 바이트는 버리고
/// `length`가 있으면 그만큼만 보낸다. 읽기가 실패하거나 `length`보다 일찍 끝나면 본문이 오류로
/// 끝난다(hyper가 연결을 끊는다). 클라이언트가 떠나면 읽기를 멈추고 `reader`를 이 스레드에서
/// 버린다.
fn stream_blocking(reader: Box<dyn Read + Send>, skip: u64, length: Option<u64>) -> Body {
    let (sender, receiver) = mpsc::channel(STREAM_CHANNEL_CHUNKS);
    tokio::task::spawn_blocking(move || {
        if let Err(err) = pump(reader, skip, length, &sender) {
            let _ = sender.blocking_send(Err(err));
        }
    });
    Body::from_stream(Chunks(receiver))
}

type Chunk = std::io::Result<Bytes>;

fn pump(
    mut reader: Box<dyn Read + Send>,
    skip: u64,
    length: Option<u64>,
    sender: &mpsc::Sender<Chunk>,
) -> std::io::Result<()> {
    let ended_early =
        || std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "model file ended early");
    if skip > 0 && std::io::copy(&mut (&mut reader).take(skip), &mut std::io::sink())? < skip {
        return Err(ended_early());
    }
    let mut remaining = length.unwrap_or(u64::MAX);
    while remaining > 0 {
        let size = usize::try_from(remaining)
            .unwrap_or(usize::MAX)
            .min(STREAM_CHUNK_BYTES);
        let mut buffer = vec![0_u8; size];
        let read = match reader.read(&mut buffer) {
            Ok(0) if length.is_some() => return Err(ended_early()),
            Ok(0) => return Ok(()),
            Ok(read) => read,
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(err) => return Err(err),
        };
        buffer.truncate(read);
        remaining -= read as u64;
        if sender.blocking_send(Ok(Bytes::from(buffer))).is_err() {
            // 클라이언트가 떠났다.
            return Ok(());
        }
    }
    Ok(())
}

struct Chunks(mpsc::Receiver<Chunk>);

impl futures_core::Stream for Chunks {
    type Item = Chunk;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Chunk>> {
        self.get_mut().0.poll_recv(cx)
    }
}

async fn serve_cached(cached: CachedModelFile, range: Option<&str>) -> Result<Response, ApiError> {
    let io_error = |err: std::io::Error| {
        tracing::error!(error = %err, path = %cached.path.display(), "failed to read cached model file");
        ApiError::Internal
    };
    let mut handle = tokio::fs::File::open(&cached.path)
        .await
        .map_err(io_error)?;
    let total = handle.metadata().await.map_err(io_error)?.len();

    let range = match range {
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
        RangeRequest::Unsatisfiable => return Ok(unsatisfiable(total)),
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
    with_headers(
        (status, body).into_response(),
        &cached.content_type,
        Some(length),
        content_range,
    )
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
