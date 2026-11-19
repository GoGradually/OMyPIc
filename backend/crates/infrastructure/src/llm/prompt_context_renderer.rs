use omypic_domain::session::{LlmPromptContext, RecommendationRecord, TurnRecord};
use omypic_domain::shared::text_utils::{java_is_blank, java_trim};

/// 세션 기억(요약, 최근 턴, 최근 추천 표현)을 이번 턴 프롬프트 앞에 붙인다.
/// 모든 LLM 제공자가 같이 쓴다.
pub struct LlmPromptContextRenderer;

impl LlmPromptContextRenderer {
    pub fn merge(user_prompt: &str, prompt_context: &LlmPromptContext) -> String {
        let prompt = java_trim(user_prompt);
        if is_empty(prompt_context) {
            return prompt.to_string();
        }
        let mut builder = String::from("# Previous context\n");
        append_summary(&mut builder, prompt_context.summary());
        append_turns(&mut builder, prompt_context.recent_turns());
        append_recommendations(&mut builder, prompt_context.recent_recommendations());
        builder.push_str("# Current input\n");
        builder.push_str(prompt);
        builder
    }
}

fn is_empty(context: &LlmPromptContext) -> bool {
    java_is_blank(context.summary())
        && context.recent_turns().is_empty()
        && context.recent_recommendations().is_empty()
}

fn append_summary(builder: &mut String, summary: &str) {
    if !java_is_blank(summary) {
        builder.push_str("Summary:\n");
        builder.push_str(summary);
        builder.push_str("\n\n");
    }
}

fn append_turns(builder: &mut String, turns: &[TurnRecord]) {
    if turns.is_empty() {
        return;
    }
    builder.push_str("Recent turns:\n");
    for (offset, turn) in turns.iter().enumerate() {
        builder.push_str(&format!(
            "{}) Q: {}\nA: {}\nFeedback summary: {}\n",
            offset + 1,
            java_trim(turn.question()),
            java_trim(turn.answer()),
            java_trim(turn.feedback_summary())
        ));
    }
    builder.push('\n');
}

fn append_recommendations(builder: &mut String, records: &[RecommendationRecord]) {
    if records.is_empty() {
        return;
    }
    builder.push_str("Recent recommendation terms:\n");
    for (offset, record) in records.iter().enumerate() {
        builder.push_str(&format!(
            "{}) Filler: {}, Adjective: {}, Adverb: {}\n",
            offset + 1,
            java_trim(record.filler_term()),
            java_trim(record.adjective_term()),
            java_trim(record.adverb_term())
        ));
    }
    builder.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_returns_trimmed_prompt_when_context_is_empty() {
        assert_eq!(
            "user",
            LlmPromptContextRenderer::merge("  user\n", &LlmPromptContext::empty())
        );
        let blank_summary = LlmPromptContext::new("   ", vec![], vec![]);
        assert_eq!(
            "user",
            LlmPromptContextRenderer::merge("user", &blank_summary)
        );
    }

    #[test]
    fn merge_renders_summary_turns_and_recommendations_in_order() {
        let context = LlmPromptContext::new(
            "earlier talk",
            vec![
                TurnRecord::new("Q1", "A1", "S1"),
                TurnRecord::new("Q2", "A2", "S2"),
            ],
            vec![RecommendationRecord::new("well", "vivid", "definitely")],
        );

        let merged = LlmPromptContextRenderer::merge("current", &context);

        assert_eq!(
            "# Previous context\n\
             Summary:\nearlier talk\n\n\
             Recent turns:\n\
             1) Q: Q1\nA: A1\nFeedback summary: S1\n\
             2) Q: Q2\nA: A2\nFeedback summary: S2\n\n\
             Recent recommendation terms:\n\
             1) Filler: well, Adjective: vivid, Adverb: definitely\n\n\
             # Current input\ncurrent",
            merged
        );
    }

    #[test]
    fn merge_skips_missing_sections() {
        let context = LlmPromptContext::new(
            "",
            vec![],
            vec![RecommendationRecord::new("well", "vivid", "definitely")],
        );

        let merged = LlmPromptContextRenderer::merge("user", &context);

        assert_eq!(
            "# Previous context\n\
             Recent recommendation terms:\n\
             1) Filler: well, Adjective: vivid, Adverb: definitely\n\n\
             # Current input\nuser",
            merged
        );
    }
}
