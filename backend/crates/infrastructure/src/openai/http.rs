//! 공유 HTTP 클라이언트와 요청 도우미. 시간 제한과 본문 한도는 Java `WebClientConfig` 값이다.

use std::error::Error as StdError;
use std::io::Read;
use std::time::Duration;

use omypic_domain::error::{AppError, AppResult};
use reqwest::blocking::{Client, RequestBuilder};

/// 연결 제한 시간. Java 쪽은 지정하지 않아서 Netty 기본값(30초)이 적용됐다.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
/// 응답을 기다리는 제한 시간. `HttpClient.responseTimeout(Duration.ofSeconds(60))`.
pub const RESPONSE_TIMEOUT: Duration = Duration::from_secs(60);
/// `ConnectionProvider.maxConnections(50)`에 대응해 호스트마다 남겨 두는 유휴 연결 수.
pub const MAX_IDLE_CONNECTIONS_PER_HOST: usize = 50;
/// `maxInMemorySize(20 * 1024 * 1024)`.
pub const MAX_BODY_BYTES: usize = 20 * 1024 * 1024;

/// 모든 OpenAI 어댑터가 나눠 쓰는 블로킹 클라이언트.
pub fn build_http_client() -> AppResult<Client> {
    build_http_client_with_response_timeout(RESPONSE_TIMEOUT)
}

/// 응답 제한 시간만 다른 클라이언트(Anthropic처럼 더 긴 제한이 필요한 쪽).
/// reqwest 블로킹 클라이언트에는 읽기 단위 제한이 없어서 요청 하나의 전체 제한 시간으로 건다.
pub fn build_http_client_with_response_timeout(response_timeout: Duration) -> AppResult<Client> {
    Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(response_timeout)
        .pool_max_idle_per_host(MAX_IDLE_CONNECTIONS_PER_HOST)
        .build()
        .map_err(|err| AppError::Internal(format!("HTTP client build failed: {}", chain(&err))))
}

/// 끝 슬래시를 떼고 경로를 붙인다(Spring `UriBuilder`처럼 `//`가 생기지 않는다).
pub(crate) fn join_url(base_url: &str, path: &str) -> String {
    format!("{}{}", base_url.trim_end_matches('/'), path)
}

/// 상태 코드와 본문. 오류 상태도 그대로 돌려주고 해석은 부르는 쪽이 한다.
pub(crate) struct HttpReply {
    pub status: u16,
    pub body: Vec<u8>,
}

impl HttpReply {
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

/// 요청을 보내지 못했거나 응답을 읽지 못한 경우(Java `WebClientRequestException` 계열).
#[derive(Debug)]
pub(crate) enum TransportError {
    Io(String),
    BodyTooLarge,
}

impl TransportError {
    pub fn into_app_error(self) -> AppError {
        match self {
            TransportError::Io(message) => AppError::Internal(message),
            TransportError::BodyTooLarge => AppError::InvalidState(format!(
                "Exceeded limit on max bytes to buffer : {MAX_BODY_BYTES}"
            )),
        }
    }
}

/// 요청을 보내고 본문을 읽는다. `limit`이 있으면 그 이상은 `BodyTooLarge`.
pub(crate) fn send(
    request: RequestBuilder,
    limit: Option<usize>,
) -> Result<HttpReply, TransportError> {
    let mut response = request
        .send()
        .map_err(|err| TransportError::Io(chain(&err)))?;
    let status = response.status().as_u16();
    let mut body = Vec::new();
    match limit {
        Some(limit) => {
            (&mut response)
                .take(limit as u64 + 1)
                .read_to_end(&mut body)
                .map_err(|err| TransportError::Io(chain(&err)))?;
            if body.len() > limit {
                return Err(TransportError::BodyTooLarge);
            }
        }
        None => {
            response
                .read_to_end(&mut body)
                .map_err(|err| TransportError::Io(chain(&err)))?;
        }
    }
    Ok(HttpReply { status, body })
}

/// 오류와 그 원인을 `": "`로 이어 붙인다. 요청 헤더(API 키)는 오류 글에 들어 있지 않다.
pub(crate) fn chain(error: &dyn StdError) -> String {
    let mut message = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        message.push_str(": ");
        message.push_str(&cause.to_string());
        source = cause.source();
    }
    message
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_url_never_doubles_the_slash() {
        assert_eq!(
            "http://h:1/v1/responses",
            join_url("http://h:1/", "/v1/responses")
        );
        assert_eq!(
            "http://h:1/v1/responses",
            join_url("http://h:1", "/v1/responses")
        );
    }

    #[test]
    fn client_builds_with_configured_limits() {
        assert_eq!(Duration::from_secs(60), RESPONSE_TIMEOUT);
        assert_eq!(20 * 1024 * 1024, MAX_BODY_BYTES);
        assert_eq!(50, MAX_IDLE_CONNECTIONS_PER_HOST);
        assert!(build_http_client().is_ok());
    }
}
