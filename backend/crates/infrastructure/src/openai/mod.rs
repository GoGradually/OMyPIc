//! OpenAI 연동: `/v1/responses`와 `/v1/chat/completions` LLM 클라이언트, STT, TTS, 키 확인.
//!
//! 이 어댑터들은 블로킹 스레드에서만 부른다(HTTP 계층이 `spawn_blocking`으로 부른다).
//! LLM/STT/TTS 어댑터는 `build_http_client(..)`(요청 하나의 전체 제한은 설정값) 하나를 나눠 쓰고,
//! 키 확인 어댑터는 `build_probe_http_client()`(응답 60초)를 쓴다.

pub(crate) mod http;
mod key_probe;
mod llm_client;
mod log_formatter;
mod parameter_policy;
mod stt_gateway;
mod tts_gateway;

pub use http::{build_http_client, build_http_client_with_limits, build_probe_http_client};
pub use key_probe::HttpApiKeyProbeAdapter;
pub use llm_client::OpenAiLlmClient;
pub use log_formatter::OpenAiLlmLogFormatter;
pub use parameter_policy::{
    Gpt5ModelParameterPolicy, LegacyChatModelParameterPolicy, OpenAiModelParameterPolicy,
    OpenAiModelParameterPolicyResolver,
};
pub use stt_gateway::OpenAiSttGateway;
pub use tts_gateway::OpenAiTtsGateway;
