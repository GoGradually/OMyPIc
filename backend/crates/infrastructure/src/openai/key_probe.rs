use std::thread;
use std::time::Duration;

use omypic_application::apikey::port::ApiKeyProbePort;
use omypic_application::feedback::policy::FeedbackModelPolicy;
use omypic_domain::error::{AppError, AppResult};
use omypic_domain::shared::text_utils::{java_is_blank, java_trim};
use reqwest::blocking::Client;
use serde_json::json;

use super::http::{self, join_url, HttpReply, TransportError};
use crate::config::{AnthropicConfig, OpenAiConfig};

const OPENAI_DEFAULT_MODEL: &str = "gpt-4o-mini";
const CHAT_COMPLETIONS_PATH: &str = "/v1/chat/completions";
const ANTHROPIC_MODELS_PATH: &str = "/v1/models/";
/// Anthropic Java SDK 3.3.0(`PrepareRequest.kt`)이 모든 요청에 넣는 기본 헤더 값.
const ANTHROPIC_VERSION: &str = "2026-10-21";
const RETRY_BASE_DELAY: Duration = Duration::from_millis(500);
const RETRY_MAX_DELAY: Duration = Duration::from_secs(8);

/// OpenAI와 Anthropic 키 확인을 모두 맡는다(Java `WebClientApiKeyProbeAdapter`).
/// OpenAI는 채팅 완성 한 번(`ping`)으로, Anthropic은 모델 조회로 키를 확인한다.
/// `http`는 `build_probe_http_client()`로 만든 것을 넘긴다(Anthropic 요청은 자기 제한 시간을 따로 건다).
pub struct HttpApiKeyProbeAdapter {
    http: Client,
    openai_base_url: String,
    anthropic_base_url: String,
    anthropic_timeout: Duration,
    anthropic_max_retries: u32,
}

impl HttpApiKeyProbeAdapter {
    pub fn new(http: Client, openai: &OpenAiConfig, anthropic: &AnthropicConfig) -> Self {
        Self {
            http,
            openai_base_url: openai.base_url.clone(),
            anthropic_base_url: anthropic.base_url.trim().trim_end_matches('/').to_string(),
            anthropic_timeout: Duration::from_secs(anthropic.timeout_seconds.max(1)),
            anthropic_max_retries: anthropic.max_retries,
        }
    }

    fn probe_open_ai(&self, api_key: &str, model: Option<&str>) -> AppResult<()> {
        let body = json!({
            "messages": [{"content": "ping", "role": "user"}],
            "model": resolve_open_ai_model(model),
            "stream": false,
            "temperature": 0.0
        });
        let url = join_url(&self.openai_base_url, CHAT_COMPLETIONS_PATH);
        let request = self
            .http
            .post(&url)
            .bearer_auth(api_key)
            .header("Content-Type", "application/json")
            .body(body.to_string());
        match http::send(request, Some(http::MAX_BODY_BYTES)) {
            Ok(reply) if reply.is_success() => Ok(()),
            Ok(reply) => Err(verification_failed(&format!(
                "{} - {}",
                reply.status,
                reply.text()
            ))),
            Err(TransportError::Io(message)) => Err(verification_failed(&format!(
                "I/O error on POST request for \"{url}\": {message}"
            ))),
            Err(other) => Err(verification_failed(&other.into_app_error().to_string())),
        }
    }

    /// 메시지를 보내는 대신 모델을 조회한다: 토큰을 쓰지 않고 키와 모델 id를 한 번에 확인한다.
    fn probe_anthropic(&self, api_key: &str, model: Option<&str>) -> AppResult<()> {
        if java_is_blank(api_key) {
            return Err(AppError::InvalidArgument(
                "Anthropic API key is required".to_string(),
            ));
        }
        let resolved_model = resolve_anthropic_model(model)?;
        let url = format!(
            "{}{}{}",
            self.anthropic_base_url,
            ANTHROPIC_MODELS_PATH,
            encode_path_segment(&resolved_model)
        );
        match self.get_with_retries(&url, api_key) {
            Ok(reply) if reply.is_success() => Ok(()),
            Ok(reply) => Err(verification_failed(&describe_anthropic_status(
                reply.status,
                &resolved_model,
            ))),
            Err(_) => Err(verification_failed("could not reach the Anthropic API")),
        }
    }

    /// Anthropic SDK처럼 408/409/429/5xx와 연결 실패를 `max_retries`번까지 다시 시도한다.
    fn get_with_retries(&self, url: &str, api_key: &str) -> Result<HttpReply, TransportError> {
        let mut attempt: u32 = 0;
        loop {
            let request = self
                .http
                .get(url)
                .timeout(self.anthropic_timeout)
                .header("Accept", "application/json")
                .header("anthropic-version", ANTHROPIC_VERSION)
                .bearer_auth(api_key);
            let outcome = http::send(request, Some(http::MAX_BODY_BYTES));
            let retryable = match &outcome {
                Ok(reply) => matches!(reply.status, 408 | 409 | 429 | 500..=599),
                Err(TransportError::Io(_)) => true,
                Err(TransportError::BodyTooLarge) => false,
            };
            if !retryable || attempt >= self.anthropic_max_retries {
                return outcome;
            }
            thread::sleep(retry_delay(attempt));
            attempt += 1;
        }
    }
}

impl ApiKeyProbePort for HttpApiKeyProbeAdapter {
    fn probe(&self, provider: &str, api_key: &str, model: Option<&str>) -> AppResult<()> {
        let normalized = FeedbackModelPolicy::normalize_provider(Some(provider))?;
        if normalized == FeedbackModelPolicy::ANTHROPIC_PROVIDER {
            return self.probe_anthropic(api_key, model);
        }
        self.probe_open_ai(api_key, model)
    }
}

fn describe_anthropic_status(status: u16, model: &str) -> String {
    match status {
        401 => "the Anthropic API key was rejected".to_string(),
        404 => format!("this key has no access to {model}"),
        other => format!("HTTP {other}"),
    }
}

fn resolve_open_ai_model(model: Option<&str>) -> String {
    let candidate = java_trim(model.unwrap_or(""));
    if java_is_blank(candidate) || candidate.to_lowercase().contains("transcribe") {
        return OPENAI_DEFAULT_MODEL.to_string();
    }
    candidate.to_string()
}

fn resolve_anthropic_model(model: Option<&str>) -> AppResult<String> {
    match model {
        Some(model) if !java_is_blank(model) => Ok(java_trim(model).to_lowercase()),
        _ => Ok(
            FeedbackModelPolicy::default_model(Some(FeedbackModelPolicy::ANTHROPIC_PROVIDER))?
                .to_string(),
        ),
    }
}

fn verification_failed(reason: &str) -> AppError {
    let message = if java_is_blank(reason) {
        "unknown"
    } else {
        reason
    };
    AppError::InvalidState(format!("Verification failed: {message}"))
}

fn retry_delay(attempt: u32) -> Duration {
    RETRY_BASE_DELAY
        .saturating_mul(2u32.saturating_pow(attempt))
        .min(RETRY_MAX_DELAY)
}

/// URL 경로 한 칸에 넣을 수 있게 예약 문자를 퍼센트 인코딩한다.
fn encode_path_segment(segment: &str) -> String {
    let mut encoded = String::with_capacity(segment.len());
    for byte in segment.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(byte as char)
            }
            other => encoded.push_str(&format!("%{other:02X}")),
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::OpenAiLoggingConfig;
    use httpmock::prelude::*;

    fn adapter(server: &MockServer) -> HttpApiKeyProbeAdapter {
        adapter_with_retries(server, 0)
    }

    fn adapter_with_retries(server: &MockServer, max_retries: u32) -> HttpApiKeyProbeAdapter {
        let base_url = format!("{}/", server.base_url());
        HttpApiKeyProbeAdapter::new(
            http::build_probe_http_client().unwrap(),
            &OpenAiConfig {
                base_url: base_url.clone(),
                responses_enabled: true,
                conversation_rebase_turns: 6,
                logging: OpenAiLoggingConfig {
                    response_preview_chars: 512,
                    full_body: false,
                    log_success_at_fine: true,
                },
            },
            &AnthropicConfig {
                base_url,
                max_tokens: 16_000,
                effort: "high".to_string(),
                timeout_seconds: 120,
                max_retries,
            },
        )
    }

    const CHAT_OK: &str = r#"{
        "id":"chatcmpl-test","object":"chat.completion","created":1730000000,"model":"gpt-4o-mini",
        "choices":[{"index":0,"message":{"role":"assistant","content":"pong"},"finish_reason":"stop"}]
    }"#;

    #[test]
    fn probe_open_ai_sends_bearer_token() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/chat/completions")
                .header("authorization", "Bearer sk-test")
                .header("content-type", "application/json")
                .json_body(json!({
                    "messages": [{"content": "ping", "role": "user"}],
                    "model": "gpt-4o-mini",
                    "stream": false,
                    "temperature": 0.0
                }));
            then.status(200)
                .header("Content-Type", "application/json")
                .body(CHAT_OK);
        });

        adapter(&server)
            .probe("openai", "sk-test", Some("gpt-4o-mini"))
            .unwrap();

        mock.assert();
    }

    #[test]
    fn probe_open_ai_defaults_blank_and_transcribe_models() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/chat/completions")
                .json_body_partial(r#"{"model":"gpt-4o-mini"}"#);
            then.status(200).body(CHAT_OK);
        });
        let adapter = adapter(&server);

        adapter.probe("openai", "sk-test", None).unwrap();
        adapter
            .probe("openai", "sk-test", Some("gpt-4o-transcribe"))
            .unwrap();

        mock.assert_hits(2);
    }

    #[test]
    fn probe_throws_on_unauthorized() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/v1/chat/completions");
            then.status(401);
        });

        let error = adapter(&server)
            .probe("openai", "sk-test", None)
            .err()
            .unwrap();

        assert_eq!(
            AppError::InvalidState("Verification failed: 401 - ".to_string()),
            error
        );
    }

    #[test]
    fn probe_open_ai_includes_status_and_body_in_failure() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/v1/chat/completions");
            then.status(401)
                .body("{\"error\":{\"message\":\"Incorrect API key provided\"}}");
        });

        let error = adapter(&server)
            .probe("openai", "sk-test", None)
            .err()
            .unwrap();

        assert_eq!(
            AppError::InvalidState(
                "Verification failed: 401 - {\"error\":{\"message\":\"Incorrect API key provided\"}}"
                    .to_string()
            ),
            error
        );
    }

    #[test]
    fn probe_open_ai_connection_failure_is_a_verification_failure_without_the_key() {
        let port = {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            listener.local_addr().unwrap().port()
        };
        let server = MockServer::start();
        let mut adapter = adapter(&server);
        adapter.openai_base_url = format!("http://127.0.0.1:{port}");

        let AppError::InvalidState(message) = adapter
            .probe("openai", "sk-test-secret", None)
            .err()
            .unwrap()
        else {
            panic!("expected InvalidState");
        };

        assert!(
            message.starts_with(&format!(
                "Verification failed: I/O error on POST request for \"http://127.0.0.1:{port}/v1/chat/completions\": "
            )),
            "{message}"
        );
        assert!(!message.contains("sk-test-secret"));
    }

    #[test]
    fn probe_throws_when_provider_unsupported() {
        let server = MockServer::start();

        let error = adapter(&server)
            .probe("gemini", "AIza-test", None)
            .err()
            .unwrap();

        assert!(matches!(error, AppError::InvalidArgument(_)), "{error:?}");
    }

    #[test]
    fn probe_anthropic_looks_up_model_with_api_key() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/v1/models/claude-opus-5-5")
                .header("authorization", "Bearer sk-ant-test")
                .header("accept", "application/json")
                .header("anthropic-version", "2026-10-21")
                .matches(|req| {
                    !req.headers
                        .as_ref()
                        .is_some_and(|h| h.iter().any(|(n, _)| n.eq_ignore_ascii_case("x-api-key")))
                });
            then.status(200)
                .header("Content-Type", "application/json")
                .body(
                    r#"{"id":"claude-opus-5-5","type":"model","display_name":"Claude Opus 5.5","created_at":"2026-01-01T00:00:00Z"}"#,
                );
        });

        adapter(&server)
            .probe("anthropic", "sk-ant-test", None)
            .unwrap();

        mock.assert();
    }

    #[test]
    fn probe_anthropic_lowercases_the_requested_model() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).path("/v1/models/claude-haiku-5-5");
            then.status(200).body("{}");
        });

        adapter(&server)
            .probe("anthropic", "sk-ant-test", Some(" Claude-Haiku-5-5 "))
            .unwrap();

        mock.assert();
    }

    #[test]
    fn probe_anthropic_throws_on_rejected_key() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/v1/models/claude-haiku-5-5");
            then.status(401)
                .header("Content-Type", "application/json")
                .body(r#"{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"}}"#);
        });

        let error = adapter(&server)
            .probe("anthropic", "sk-ant-bad", Some("claude-haiku-5-5"))
            .err()
            .unwrap();

        assert_eq!(
            AppError::InvalidState(
                "Verification failed: the Anthropic API key was rejected".to_string()
            ),
            error
        );
    }

    #[test]
    fn probe_anthropic_reports_missing_model_access_and_other_statuses() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/v1/models/claude-opus-5-5");
            then.status(404).body("{}");
        });
        server.mock(|when, then| {
            when.method(GET).path("/v1/models/claude-sonnet-5-5");
            then.status(403).body("{}");
        });
        let adapter = adapter(&server);

        assert_eq!(
            AppError::InvalidState(
                "Verification failed: this key has no access to claude-opus-5-5".to_string()
            ),
            adapter.probe("anthropic", "k", None).err().unwrap()
        );
        assert_eq!(
            AppError::InvalidState("Verification failed: HTTP 403".to_string()),
            adapter
                .probe("anthropic", "k", Some("claude-sonnet-5-5"))
                .err()
                .unwrap()
        );
    }

    #[test]
    fn probe_anthropic_retries_server_errors_up_to_max_retries() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).path("/v1/models/claude-opus-5-5");
            then.status(529).body("{}");
        });

        let error = adapter_with_retries(&server, 1)
            .probe("anthropic", "k", None)
            .err()
            .unwrap();

        mock.assert_hits(2);
        assert_eq!(
            AppError::InvalidState("Verification failed: HTTP 529".to_string()),
            error
        );
    }

    #[test]
    fn probe_anthropic_unreachable_host_is_a_verification_failure() {
        let port = {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            listener.local_addr().unwrap().port()
        };
        let server = MockServer::start();
        let mut adapter = adapter(&server);
        adapter.anthropic_base_url = format!("http://127.0.0.1:{port}");

        let error = adapter.probe("anthropic", "k", None).err().unwrap();

        assert_eq!(
            AppError::InvalidState(
                "Verification failed: could not reach the Anthropic API".to_string()
            ),
            error
        );
    }

    #[test]
    fn probe_anthropic_requires_a_key() {
        let server = MockServer::start();

        let error = adapter(&server)
            .probe("anthropic", "  ", None)
            .err()
            .unwrap();

        assert_eq!(
            AppError::InvalidArgument("Anthropic API key is required".to_string()),
            error
        );
    }

    #[test]
    fn retry_delay_doubles_and_caps() {
        assert_eq!(Duration::from_millis(500), retry_delay(0));
        assert_eq!(Duration::from_secs(1), retry_delay(1));
        assert_eq!(Duration::from_secs(8), retry_delay(10));
        assert_eq!(Duration::from_secs(8), retry_delay(40));
    }

    #[test]
    fn encode_path_segment_escapes_reserved_characters() {
        assert_eq!("claude-opus-5-5", encode_path_segment("claude-opus-5-5"));
        assert_eq!("a%2Fb%20c", encode_path_segment("a/b c"));
    }
}
