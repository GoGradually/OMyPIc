use omypic_application::stt::model::VadSettings;
use omypic_application::stt::port::SttGateway;
use omypic_domain::error::{AppError, AppResult};
use reqwest::blocking::multipart::{Form, Part};
use reqwest::blocking::Client;
use serde_json::Value;

use super::http::{self, join_url, MAX_BODY_BYTES};
use crate::config::OpenAiConfig;

const TRANSCRIPTIONS_PATH: &str = "/v1/audio/transcriptions";
const TRANSLATIONS_PATH: &str = "/v1/audio/translations";
const AUDIO_FILE_NAME: &str = "audio.wav";
const AUDIO_MIME: &str = "audio/x-wav";
const TEXT_PART_MIME: &str = "text/plain;charset=UTF-8";

/// OpenAI 음성 인식. Spring AI `OpenAiAudioApi`가 보내던 multipart 요청을 그대로 보낸다:
/// 파트 `file`(파일 이름 `audio.wav`), `model`, `response_format=json`.
pub struct OpenAiSttGateway {
    http: Client,
    base_url: String,
}

impl OpenAiSttGateway {
    pub fn new(http: Client, config: &OpenAiConfig) -> Self {
        Self {
            http,
            base_url: config.base_url.clone(),
        }
    }
}

impl SttGateway for OpenAiSttGateway {
    fn transcribe(
        &self,
        file_bytes: &[u8],
        model: &str,
        api_key: &str,
        translate: bool,
        _vad_settings: &VadSettings,
    ) -> AppResult<String> {
        let file = Part::bytes(file_bytes.to_vec())
            .file_name(AUDIO_FILE_NAME)
            .mime_str(AUDIO_MIME)
            .map_err(|err| AppError::Internal(err.to_string()))?;
        let form = Form::new()
            .part("file", file)
            .part("model", text_part(model)?)
            .part("response_format", text_part("json")?);
        let path = if translate {
            TRANSLATIONS_PATH
        } else {
            TRANSCRIPTIONS_PATH
        };
        let request = self
            .http
            .post(join_url(&self.base_url, path))
            .bearer_auth(api_key)
            .multipart(form);
        let reply = http::send(request, Some(MAX_BODY_BYTES)).map_err(|e| e.into_app_error())?;
        if !reply.is_success() {
            return Err(AppError::Internal(format!(
                "{} - {}",
                reply.status,
                reply.text()
            )));
        }
        parse_text(&reply.text())
    }
}

fn text_part(value: &str) -> AppResult<Part> {
    Part::text(value.to_string())
        .mime_str(TEXT_PART_MIME)
        .map_err(|err| AppError::Internal(err.to_string()))
}

/// 응답 JSON의 `text`. 없거나 null이면 빈 문자열.
fn parse_text(body: &str) -> AppResult<String> {
    if body.trim().is_empty() {
        return Ok(String::new());
    }
    let root: Value = serde_json::from_str(body)
        .map_err(|err| AppError::Internal(format!("Error while extracting response: {err}")))?;
    Ok(match root.get("text") {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(text)) => text.clone(),
        Some(other) => other.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::OpenAiLoggingConfig;
    use httpmock::prelude::*;

    fn gateway(server: &MockServer) -> OpenAiSttGateway {
        OpenAiSttGateway::new(
            http::build_http_client(std::time::Duration::from_secs(300)).unwrap(),
            &OpenAiConfig {
                base_url: format!("{}/", server.base_url()),
                responses_enabled: true,
                conversation_rebase_turns: 6,
                timeout_seconds: 300,
                logging: OpenAiLoggingConfig {
                    response_preview_chars: 512,
                    full_body: false,
                    log_success_at_fine: true,
                },
            },
        )
    }

    fn vad() -> VadSettings {
        VadSettings {
            prefix_padding_ms: 111,
            silence_duration_ms: 222,
            threshold: 0.7,
        }
    }

    #[test]
    fn transcribe_calls_transcriptions_endpoint_and_parses_text() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/audio/transcriptions")
                .header("authorization", "Bearer api-key")
                .matches(|req| {
                    req.headers.as_ref().is_some_and(|headers| {
                        headers.iter().any(|(name, value)| {
                            name.eq_ignore_ascii_case("content-type")
                                && value.starts_with("multipart/form-data; boundary=")
                        })
                    })
                })
                .body_contains("name=\"model\"")
                .body_contains("whisper-1")
                .body_contains("name=\"response_format\"")
                .body_contains("json")
                .body_contains("name=\"file\"; filename=\"audio.wav\"");
            then.status(200)
                .header("Content-Type", "application/json")
                .body("{\"text\":\"hello\"}");
        });

        let text = gateway(&server)
            .transcribe(&[1, 2, 3], "whisper-1", "api-key", false, &vad())
            .unwrap();

        assert_eq!("hello", text);
        mock.assert();
    }

    #[test]
    fn transcribe_sends_only_file_model_and_response_format_parts() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/audio/transcriptions")
                .body_contains("name=\"file\"")
                .body_contains("name=\"model\"")
                .body_contains("name=\"response_format\"")
                .matches(|req| {
                    let body = String::from_utf8_lossy(req.body.as_deref().unwrap_or_default())
                        .into_owned();
                    body.matches("Content-Disposition: form-data").count() == 3
                        && !body.contains("name=\"language\"")
                        && !body.contains("name=\"temperature\"")
                        && !body.contains("name=\"prompt\"")
                });
            then.status(200).body("{\"text\":\"ok\"}");
        });

        gateway(&server)
            .transcribe(&[1, 2, 3], "whisper-1", "api-key", false, &vad())
            .unwrap();

        mock.assert();
    }

    #[test]
    fn transcribe_calls_translations_endpoint_when_translate_true() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/audio/translations")
                .header("authorization", "Bearer api-key");
            then.status(200)
                .header("Content-Type", "application/json")
                .body("{\"text\":\"translated\"}");
        });

        let text = gateway(&server)
            .transcribe(
                &[9],
                "whisper-1",
                "api-key",
                true,
                &VadSettings {
                    prefix_padding_ms: 1,
                    silence_duration_ms: 2,
                    threshold: 0.5,
                },
            )
            .unwrap();

        assert_eq!("translated", text);
        mock.assert();
    }

    #[test]
    fn transcribe_returns_empty_string_when_text_missing() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/v1/audio/transcriptions");
            then.status(200)
                .header("Content-Type", "application/json")
                .body("{}");
        });

        let text = gateway(&server)
            .transcribe(&[1], "whisper-1", "api-key", false, &vad())
            .unwrap();

        assert_eq!("", text);
    }

    #[test]
    fn transcribe_reports_http_errors_as_status_dash_body() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/v1/audio/transcriptions");
            then.status(401).body("{\"error\":\"bad key\"}");
        });

        let error = gateway(&server)
            .transcribe(&[1], "whisper-1", "api-key", false, &vad())
            .err()
            .unwrap();

        assert_eq!(
            AppError::Internal("401 - {\"error\":\"bad key\"}".to_string()),
            error
        );
    }

    #[test]
    fn transcribe_sends_empty_file_part_for_empty_audio() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/audio/transcriptions")
                .body_contains("filename=\"audio.wav\"");
            then.status(200).body("{\"text\":\"\"}");
        });

        let text = gateway(&server)
            .transcribe(&[], "whisper-1", "api-key", false, &vad())
            .unwrap();

        assert_eq!("", text);
        mock.assert();
    }
}
