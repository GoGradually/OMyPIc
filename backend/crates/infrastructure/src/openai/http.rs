//! 공유 HTTP 클라이언트와 요청 도우미. 시간 제한과 본문 한도는 Java `WebClientConfig` 값이다.

use std::error::Error as StdError;
use std::io::Read;
use std::time::Duration;

use omypic_domain::error::{AppError, AppResult};
use reqwest::blocking::{Client, RequestBuilder};

/// 연결 제한 시간. Java 쪽은 지정하지 않아서 Netty 기본값(30초)이 적용됐다.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
/// 키 확인용 클라이언트의 응답 제한 시간. `WebClientConfig`의 `responseTimeout(60s)` 값이다.
/// LLM, STT, TTS 요청에는 적용하지 않는다(Java에서 그 요청들은 이 제한을 쓰지 않았다).
pub const PROBE_RESPONSE_TIMEOUT: Duration = Duration::from_secs(60);
/// `ConnectionProvider.maxConnections(50)`에 대응해 호스트마다 남겨 두는 유휴 연결 수.
pub const MAX_IDLE_CONNECTIONS_PER_HOST: usize = 50;
/// `maxInMemorySize(20 * 1024 * 1024)`.
pub const MAX_BODY_BYTES: usize = 20 * 1024 * 1024;

/// LLM, STT, TTS 어댑터가 나눠 쓰는 블로킹 클라이언트: 연결 제한 시간만 있고 전체 제한은 없다.
pub fn build_http_client() -> AppResult<Client> {
    build_http_client_with_limits(CONNECT_TIMEOUT, None)
}

/// 키 확인 어댑터용 클라이언트: 연결 30초에 응답 60초.
/// reqwest 블로킹 클라이언트에는 읽기 단위 제한이 없어서 요청 하나의 전체 제한 시간으로 건다.
pub fn build_probe_http_client() -> AppResult<Client> {
    build_http_client_with_limits(CONNECT_TIMEOUT, Some(PROBE_RESPONSE_TIMEOUT))
}

/// 제한 시간을 직접 정하는 클라이언트(테스트와, 더 긴 제한이 필요한 쪽).
pub fn build_http_client_with_limits(
    connect_timeout: Duration,
    response_timeout: Option<Duration>,
) -> AppResult<Client> {
    Client::builder()
        .connect_timeout(connect_timeout)
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
    pub headers: reqwest::header::HeaderMap,
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
    let headers = response.headers().clone();
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
    Ok(HttpReply {
        status,
        headers,
        body,
    })
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
        assert_eq!(Duration::from_secs(60), PROBE_RESPONSE_TIMEOUT);
        assert_eq!(20 * 1024 * 1024, MAX_BODY_BYTES);
        assert_eq!(50, MAX_IDLE_CONNECTIONS_PER_HOST);
        assert!(build_http_client().is_ok());
        assert!(build_probe_http_client().is_ok());
    }

    use std::io::Write;
    use std::net::TcpListener;

    /// 요청을 다 읽고 `delay` 뒤에 `body`를 JSON으로 돌려주는 가짜 서버. 연결마다 스레드를 쓴다.
    fn delayed_server(delay: Duration, body: String) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let body = body.clone();
                std::thread::spawn(move || {
                    let mut received = Vec::new();
                    let mut chunk = [0u8; 4096];
                    loop {
                        let Ok(read) = stream.read(&mut chunk) else {
                            return;
                        };
                        if read == 0 {
                            return;
                        }
                        received.extend_from_slice(&chunk[..read]);
                        let text = String::from_utf8_lossy(&received).into_owned();
                        if let Some(end) = text.find("\r\n\r\n") {
                            let length = text[..end]
                                .lines()
                                .find_map(|line| {
                                    line.to_ascii_lowercase()
                                        .strip_prefix("content-length:")
                                        .and_then(|v| v.trim().parse::<usize>().ok())
                                })
                                .unwrap_or(0);
                            if received.len() >= end + 4 + length {
                                break;
                            }
                        }
                    }
                    std::thread::sleep(delay);
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = stream.write_all(response.as_bytes());
                });
            }
        });
        format!("http://{address}")
    }

    fn openai_config(base_url: String) -> crate::config::OpenAiConfig {
        crate::config::OpenAiConfig {
            base_url,
            responses_enabled: true,
            conversation_rebase_turns: 6,
            logging: crate::config::OpenAiLoggingConfig {
                response_preview_chars: 512,
                full_body: false,
                log_success_at_fine: true,
            },
        }
    }

    #[test]
    fn llm_call_has_no_total_deadline_but_the_probe_does() {
        use omypic_application::apikey::port::ApiKeyProbePort;
        use omypic_application::feedback::port::LlmClient;
        use omypic_domain::session::LlmConversationState;

        // 시험용으로 줄인 제한: 응답은 0.9초 뒤에 오고, 키 확인 제한은 0.3초다.
        let delay = Duration::from_millis(900);
        let probe_limit = Duration::from_millis(300);
        let reply = serde_json::json!({
            "id": "resp-1",
            "conversation": "conv-1",
            "output_text": "ok",
            "choices": [{"message": {"content": "pong"}}]
        })
        .to_string();
        let base_url = delayed_server(delay, reply);
        let config = openai_config(base_url.clone());

        let llm_http = build_http_client_with_limits(CONNECT_TIMEOUT, None).unwrap();
        let llm = crate::openai::OpenAiLlmClient::new(llm_http, &config);
        let state = llm
            .bootstrap("k", "gpt-4o-mini", "sys", &LlmConversationState::empty())
            .unwrap();
        assert_eq!("conv-1", state.conversation_id());

        let probe_http = build_http_client_with_limits(CONNECT_TIMEOUT, Some(probe_limit)).unwrap();
        let anthropic = crate::config::AnthropicConfig {
            base_url,
            max_tokens: 16_000,
            effort: "high".to_string(),
            timeout_seconds: 120,
            max_retries: 0,
        };
        let probe = crate::openai::HttpApiKeyProbeAdapter::new(probe_http, &config, &anthropic);
        let error = probe.probe("openai", "k", None).err().unwrap();
        let AppError::InvalidState(message) = error else {
            panic!("expected InvalidState");
        };
        assert!(
            message.starts_with("Verification failed: I/O error on POST request for"),
            "{message}"
        );

        // 같은 지연이 제한 없는 클라이언트의 키 확인은 통과한다.
        let unlimited = build_http_client_with_limits(CONNECT_TIMEOUT, None).unwrap();
        let probe = crate::openai::HttpApiKeyProbeAdapter::new(unlimited, &config, &anthropic);
        probe.probe("openai", "k", None).unwrap();
    }
}
