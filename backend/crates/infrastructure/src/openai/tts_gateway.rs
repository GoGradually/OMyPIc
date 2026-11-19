use omypic_application::voice::port::TtsGateway;
use omypic_domain::error::{AppError, AppResult};
use omypic_domain::shared::text_utils::java_is_blank;
use reqwest::blocking::Client;
use serde_json::json;

use super::http::{self, join_url};
use crate::config::OpenAiConfig;

const SPEECH_PATH: &str = "/v1/audio/speech";

/// OpenAI 음성 합성. `{"model","input","voice","response_format":"wav"}`를 보내고 WAV 바이트를 받는다.
pub struct OpenAiTtsGateway {
    http: Client,
    base_url: String,
}

impl OpenAiTtsGateway {
    pub fn new(http: Client, config: &OpenAiConfig) -> Self {
        Self {
            http,
            base_url: config.base_url.clone(),
        }
    }
}

impl TtsGateway for OpenAiTtsGateway {
    fn synthesize(
        &self,
        api_key: &str,
        model: &str,
        voice: &str,
        text: &str,
    ) -> AppResult<Vec<u8>> {
        if java_is_blank(text) {
            return Ok(Vec::new());
        }
        let body = json!({
            "model": model,
            "input": text,
            "voice": voice,
            "response_format": "wav"
        });
        let request = self
            .http
            .post(join_url(&self.base_url, SPEECH_PATH))
            .bearer_auth(api_key)
            .header("Content-Type", "application/json")
            .body(body.to_string());
        // 음성 본문은 WebClient 버퍼 한도를 거치지 않던 경로라서 한도를 두지 않는다.
        let reply = http::send(request, None).map_err(|err| err.into_app_error())?;
        if !reply.is_success() {
            return Err(AppError::Internal(format!(
                "{} - {}",
                reply.status,
                reply.text()
            )));
        }
        Ok(reply.body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::OpenAiLoggingConfig;
    use httpmock::prelude::*;

    fn gateway(server: &MockServer) -> OpenAiTtsGateway {
        OpenAiTtsGateway::new(
            http::build_http_client().unwrap(),
            &OpenAiConfig {
                base_url: format!("{}/", server.base_url()),
                responses_enabled: true,
                conversation_rebase_turns: 6,
                logging: OpenAiLoggingConfig {
                    response_preview_chars: 512,
                    full_body: false,
                    log_success_at_fine: true,
                },
            },
        )
    }

    #[test]
    fn synthesize_posts_speech_request_and_returns_audio_bytes() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/audio/speech")
                .header("authorization", "Bearer api-key")
                .header("content-type", "application/json")
                .json_body(json!({
                    "model": "gpt-4o-mini-tts",
                    "input": "hello there",
                    "voice": "alloy",
                    "response_format": "wav"
                }));
            then.status(200)
                .header("Content-Type", "audio/wav")
                .body([82u8, 73, 70, 70, 0, 255]);
        });

        let audio = gateway(&server)
            .synthesize("api-key", "gpt-4o-mini-tts", "alloy", "hello there")
            .unwrap();

        assert_eq!(vec![82u8, 73, 70, 70, 0, 255], audio);
        mock.assert();
    }

    #[test]
    fn synthesize_returns_empty_bytes_for_blank_text_without_a_request() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path("/v1/audio/speech");
            then.status(200).body("x");
        });
        let gateway = gateway(&server);

        assert!(gateway.synthesize("k", "m", "v", "").unwrap().is_empty());
        assert!(gateway
            .synthesize("k", "m", "v", " \n ")
            .unwrap()
            .is_empty());
        mock.assert_hits(0);
    }

    #[test]
    fn synthesize_reports_http_errors_as_status_dash_body() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/v1/audio/speech");
            then.status(429).body("slow down");
        });

        let error = gateway(&server)
            .synthesize("api-key", "m", "v", "hi")
            .err()
            .unwrap();

        assert_eq!(AppError::Internal("429 - slow down".to_string()), error);
    }
}
