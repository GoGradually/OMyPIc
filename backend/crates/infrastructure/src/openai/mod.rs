//! OpenAI 연동: `/v1/responses`와 `/v1/chat/completions` LLM 클라이언트, STT, TTS, 키 확인.
//!
//! 이 어댑터들은 블로킹 스레드에서만 부른다(HTTP 계층이 `spawn_blocking`으로 부른다).
//! `reqwest::blocking::Client` 하나를 만들어 모든 어댑터가 나눠 쓴다.

mod http;
mod key_probe;
mod llm_client;
mod log_formatter;
mod parameter_policy;
mod stt_gateway;
mod tts_gateway;

pub use http::{build_http_client, build_http_client_with_response_timeout};
pub use key_probe::HttpApiKeyProbeAdapter;
pub use llm_client::OpenAiLlmClient;
pub use log_formatter::OpenAiLlmLogFormatter;
pub use parameter_policy::{
    Gpt5ModelParameterPolicy, LegacyChatModelParameterPolicy, OpenAiModelParameterPolicy,
    OpenAiModelParameterPolicyResolver,
};
pub use stt_gateway::OpenAiSttGateway;
pub use tts_gateway::OpenAiTtsGateway;
