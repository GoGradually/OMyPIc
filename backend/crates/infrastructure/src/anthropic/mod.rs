//! Anthropic 연동: Messages API로 피드백을 만드는 `LlmClient`.
//!
//! Java는 공식 SDK(`com.anthropic:anthropic-java:3.3.0`)를 썼고, 여기서는 같은 요청을
//! `reqwest::blocking`으로 직접 보낸다. 이 어댑터는 블로킹 스레드에서만 부른다.

mod llm_client;

pub use llm_client::{build_anthropic_http_client, AnthropicLlmClient, Sleeper};
