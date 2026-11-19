use omypic_domain::error::AppResult;
use omypic_domain::feedback::Feedback;
use omypic_domain::session::{LlmConversationState, LlmPromptContext};

use omypic_domain::shared::text_utils::java_trim;

/// 모델이 만든 피드백과 다음 턴에 쓸 상태. 생성자가 Java record와 같이 값을 정규화한다.
#[derive(Debug, Clone)]
pub struct LlmGenerateResult {
    pub feedback: Feedback,
    pub schema_fallback_reasons: Vec<String>,
    pub conversation_state: LlmConversationState,
    pub summary_for_next_turn: String,
}

impl LlmGenerateResult {
    pub fn new(
        feedback: Option<Feedback>,
        schema_fallback_reasons: Option<Vec<String>>,
        conversation_state: Option<LlmConversationState>,
        summary_for_next_turn: Option<&str>,
    ) -> Self {
        Self {
            feedback: feedback.unwrap_or_else(|| Feedback::of_points("", &[], &[], "", &[])),
            schema_fallback_reasons: schema_fallback_reasons.unwrap_or_default(),
            conversation_state: conversation_state.unwrap_or_else(LlmConversationState::empty),
            summary_for_next_turn: java_trim(summary_for_next_turn.unwrap_or("")).to_string(),
        }
    }
}

pub trait LlmClient: Send + Sync {
    fn provider(&self) -> String;

    /// 대화를 제공자 서버가 아니라 이쪽이 들고 있으면 `true`: 매 호출에 시스템 프롬프트 전체를
    /// 실어야 하고 bootstrap은 건너뛴다. 서버가 대화를 들고 있는 제공자(OpenAI Responses)는
    /// 기본값을 쓴다.
    fn is_stateless(&self) -> bool {
        false
    }

    fn bootstrap(
        &self,
        api_key: &str,
        model: &str,
        system_prompt: &str,
        conversation_state: &LlmConversationState,
    ) -> AppResult<LlmConversationState>;

    fn generate(
        &self,
        api_key: &str,
        model: &str,
        system_prompt: &str,
        user_prompt: &str,
        conversation_state: &LlmConversationState,
        prompt_context: &LlmPromptContext,
    ) -> AppResult<LlmGenerateResult>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn llm_generate_result_normalizes_missing_values() {
        let result = LlmGenerateResult::new(None, None, None, Some("  summary \u{0001}"));

        assert_eq!("", result.feedback.summary());
        assert!(result.schema_fallback_reasons.is_empty());
        assert_eq!(LlmConversationState::empty(), result.conversation_state);
        assert_eq!("summary", result.summary_for_next_turn);
    }
}
