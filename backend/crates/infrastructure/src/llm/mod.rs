//! LLM 제공자가 함께 쓰는 부분: 피드백 JSON 해석과 세션 기억 프롬프트 렌더링.

mod feedback_json_parser;
mod prompt_context_renderer;

pub use feedback_json_parser::FeedbackJsonParser;
pub(crate) use feedback_json_parser::{as_text, path, path_index};
pub use prompt_context_renderer::LlmPromptContextRenderer;
