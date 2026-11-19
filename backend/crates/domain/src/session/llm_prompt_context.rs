use crate::shared::text_utils::java_trim;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnRecord {
    question: String,
    answer: String,
    feedback_summary: String,
}

impl TurnRecord {
    pub fn new(question: &str, answer: &str, feedback_summary: &str) -> Self {
        Self {
            question: java_trim(question).to_string(),
            answer: java_trim(answer).to_string(),
            feedback_summary: java_trim(feedback_summary).to_string(),
        }
    }

    pub fn question(&self) -> &str {
        &self.question
    }

    pub fn answer(&self) -> &str {
        &self.answer
    }

    pub fn feedback_summary(&self) -> &str {
        &self.feedback_summary
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecommendationRecord {
    filler_term: String,
    adjective_term: String,
    adverb_term: String,
}

impl RecommendationRecord {
    pub fn new(filler_term: &str, adjective_term: &str, adverb_term: &str) -> Self {
        Self {
            filler_term: java_trim(filler_term).to_string(),
            adjective_term: java_trim(adjective_term).to_string(),
            adverb_term: java_trim(adverb_term).to_string(),
        }
    }

    pub fn filler_term(&self) -> &str {
        &self.filler_term
    }

    pub fn adjective_term(&self) -> &str {
        &self.adjective_term
    }

    pub fn adverb_term(&self) -> &str {
        &self.adverb_term
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LlmPromptContext {
    summary: String,
    recent_turns: Vec<TurnRecord>,
    recent_recommendations: Vec<RecommendationRecord>,
}

impl LlmPromptContext {
    pub fn new(
        summary: &str,
        recent_turns: Vec<TurnRecord>,
        recent_recommendations: Vec<RecommendationRecord>,
    ) -> Self {
        Self {
            summary: java_trim(summary).to_string(),
            recent_turns,
            recent_recommendations,
        }
    }

    pub fn empty() -> Self {
        Self::default()
    }

    pub fn summary(&self) -> &str {
        &self.summary
    }

    pub fn recent_turns(&self) -> &[TurnRecord] {
        &self.recent_turns
    }

    pub fn recent_recommendations(&self) -> &[RecommendationRecord] {
        &self.recent_recommendations
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_trim_their_values() {
        let context = LlmPromptContext::new(
            "  summary ",
            vec![TurnRecord::new(" q ", "a ", " s")],
            vec![RecommendationRecord::new(" well", "vivid ", " clearly ")],
        );

        assert_eq!("summary", context.summary());
        assert_eq!("q", context.recent_turns()[0].question());
        assert_eq!("a", context.recent_turns()[0].answer());
        assert_eq!("s", context.recent_turns()[0].feedback_summary());
        assert_eq!("well", context.recent_recommendations()[0].filler_term());
        assert_eq!(
            "vivid",
            context.recent_recommendations()[0].adjective_term()
        );
        assert_eq!("clearly", context.recent_recommendations()[0].adverb_term());
        assert_eq!(LlmPromptContext::default(), LlmPromptContext::empty());
    }
}
