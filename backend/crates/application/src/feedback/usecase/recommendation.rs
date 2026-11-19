//! 추천 표현(filler/adjective/adverb)의 평가, 재생성(repair)과 최소 대체값.

use omypic_domain::feedback::{
    FeedbackConstraints, FeedbackLanguage, RecommendationDetail, Recommendations,
};
use omypic_domain::session::{LlmConversationState, LlmPromptContext};
use omypic_domain::shared::text_utils::{java_is_blank, java_trim};

use super::prompts::{build_recommendation_system_prompt, build_recommendation_user_prompt};
use super::{failure_message, FeedbackUseCase, TurnRequest};
use crate::feedback::model::FeedbackCommand;
use crate::feedback::port::LlmClient;

/// 최근 턴들의 추천 term. 정규화(trim, 소문자)하고 처음 나온 순서대로 중복 없이 둔다.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct RecommendationHistory {
    pub(super) filler_terms: Vec<String>,
    pub(super) adjective_terms: Vec<String>,
    pub(super) adverb_terms: Vec<String>,
}

impl RecommendationHistory {
    pub(super) fn from_prompt_context(prompt_context: &LlmPromptContext) -> Self {
        let mut history = Self::default();
        for record in prompt_context.recent_recommendations() {
            append_term(&mut history.filler_terms, record.filler_term());
            append_term(&mut history.adjective_terms, record.adjective_term());
            append_term(&mut history.adverb_terms, record.adverb_term());
        }
        history
    }
}

fn append_term(terms: &mut Vec<String>, term: &str) {
    let normalized = normalize_term(term);
    if !java_is_blank(&normalized) && !terms.contains(&normalized) {
        terms.push(normalized);
    }
}

/// Java `term.trim().toLowerCase(Locale.ROOT)`.
fn normalize_term(term: &str) -> String {
    java_trim(term).to_lowercase()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct RecommendationEvaluation {
    pub(super) missing_count: u32,
    pub(super) duplicate_count: u32,
}

impl RecommendationEvaluation {
    pub(super) fn of(recommendations: &Recommendations, history: &RecommendationHistory) -> Self {
        let mut evaluation = Self {
            missing_count: 0,
            duplicate_count: 0,
        };
        let slots = [
            (recommendations.filler(), &history.filler_terms),
            (recommendations.adjective(), &history.adjective_terms),
            (recommendations.adverb(), &history.adverb_terms),
        ];
        for (detail, terms) in slots {
            if is_recommendation_missing(detail) {
                evaluation.missing_count += 1;
            } else if is_duplicate_term(detail.term(), terms) {
                evaluation.duplicate_count += 1;
            }
        }
        evaluation
    }

    /// 빠진 항목이 있거나 둘 이상이 최근 추천과 겹치면 엄격한 중복 회피로 한 번 더 만든다.
    pub(super) fn should_attempt_repair(&self) -> bool {
        self.missing_count > 0 || self.duplicate_count >= 2
    }

    /// 빠진 항목이 적은 쪽, 같으면 중복이 적은 쪽이 낫다. 완전히 같으면 낫지 않다.
    pub(super) fn is_better_than(&self, baseline: &Self) -> bool {
        if self.missing_count != baseline.missing_count {
            return self.missing_count < baseline.missing_count;
        }
        self.duplicate_count < baseline.duplicate_count
    }
}

fn is_recommendation_missing(detail: &RecommendationDetail) -> bool {
    java_is_blank(detail.term()) && java_is_blank(detail.usage())
}

fn is_duplicate_term(term: &str, history: &[String]) -> bool {
    if history.is_empty() {
        return false;
    }
    let normalized = normalize_term(term);
    !java_is_blank(&normalized) && history.contains(&normalized)
}

pub(super) fn empty_recommendations() -> Recommendations {
    Recommendations::new(
        RecommendationDetail::new("", ""),
        RecommendationDetail::new("", ""),
        RecommendationDetail::new("", ""),
    )
}

/// 빈 항목을 최소 대체 안내문으로 채운다. 하나라도 채웠으면 `true`도 돌려준다.
pub(super) fn ensure_recommendation_completeness(
    recommendations: &Recommendations,
    language: &FeedbackLanguage,
) -> (Recommendations, bool) {
    let mut fallback_applied = false;
    let mut ensure = |detail: &RecommendationDetail, category: Category| {
        if !java_is_blank(detail.term()) || !java_is_blank(detail.usage()) {
            return detail.clone();
        }
        fallback_applied = true;
        RecommendationDetail::new("", minimal_fallback_usage(language, category))
    };
    let filler = ensure(recommendations.filler(), Category::Filler);
    let adjective = ensure(recommendations.adjective(), Category::Adjective);
    let adverb = ensure(recommendations.adverb(), Category::Adverb);
    (
        Recommendations::new(filler, adjective, adverb),
        fallback_applied,
    )
}

#[derive(Debug, Clone, Copy)]
enum Category {
    Filler,
    Adjective,
    Adverb,
}

fn minimal_fallback_usage(language: &FeedbackLanguage, category: Category) -> &'static str {
    if language.is_english() {
        return match category {
            Category::Filler => {
                "No safe filler extracted in this turn. Regenerate with the next answer context."
            }
            Category::Adjective => {
                "No safe adjective extracted in this turn. Regenerate with the next answer context."
            }
            Category::Adverb => {
                "No safe adverb extracted in this turn. Regenerate with the next answer context."
            }
        };
    }
    match category {
        Category::Filler => {
            "이번 턴에서 자연스러운 필러를 확정하지 못했습니다. 다음 답변 문맥에서 재생성합니다."
        }
        Category::Adjective => {
            "이번 턴에서 자연스러운 형용사를 확정하지 못했습니다. 다음 답변 문맥에서 재생성합니다."
        }
        Category::Adverb => {
            "이번 턴에서 자연스러운 부사를 확정하지 못했습니다. 다음 답변 문맥에서 재생성합니다."
        }
    }
}

/// 추천 표현 전용 요청 하나에 필요한 값.
pub(super) struct RecommendationRequest<'a> {
    pub(super) api_key: &'a str,
    pub(super) command: &'a FeedbackCommand,
    pub(super) client: &'a dyn LlmClient,
    pub(super) provider: &'a str,
    pub(super) constraints: &'a FeedbackConstraints,
    pub(super) turn: &'a TurnRequest<'a>,
    pub(super) history: &'a RecommendationHistory,
}

impl FeedbackUseCase {
    /// 본 피드백의 추천 표현을 추천 전용 요청의 후보와 비교해 더 나은 쪽을 고르고, 여전히 빠지거나
    /// 겹치면 엄격한 요청으로 한 번 더 고친다. 남은 빈 항목은 최소 대체 안내문으로 채운다.
    pub(super) fn select_recommendations(
        &self,
        request: &RecommendationRequest<'_>,
        base: &Recommendations,
    ) -> Recommendations {
        let mut selected = base.clone();
        let mut selected_evaluation = RecommendationEvaluation::of(&selected, request.history);

        let candidate = self.generate_recommendation_candidate(request, false);
        let candidate_evaluation = RecommendationEvaluation::of(&candidate, request.history);
        if candidate_evaluation.is_better_than(&selected_evaluation) {
            selected = candidate;
            selected_evaluation = candidate_evaluation;
        }

        if selected_evaluation.should_attempt_repair() {
            self.metrics.increment_recommendation_repair_attempt();
            let repaired = self.generate_recommendation_candidate(request, true);
            let repaired_evaluation = RecommendationEvaluation::of(&repaired, request.history);
            if repaired_evaluation.is_better_than(&selected_evaluation) {
                selected = repaired;
                self.metrics.increment_recommendation_repair_success();
            }
        }

        let (completed, minimal_fallback_applied) =
            ensure_recommendation_completeness(&selected, request.turn.language);
        if minimal_fallback_applied {
            self.metrics.increment_recommendation_minimal_fallback();
        }
        if RecommendationEvaluation::of(&completed, request.history).duplicate_count > 0 {
            self.metrics.increment_recommendation_duplicate_detected();
        }
        completed
    }

    /// 대화 상태 없이 추천 표현만 다시 요청한다. 실패하면 빈 추천을 돌려준다.
    fn generate_recommendation_candidate(
        &self,
        request: &RecommendationRequest<'_>,
        strict_deduplication: bool,
    ) -> Recommendations {
        let turn = request.turn;
        let system_prompt = build_recommendation_system_prompt(
            turn.language.value(),
            turn.contexts,
            strict_deduplication,
        );
        let user_prompt = build_recommendation_user_prompt(
            turn.question_text,
            turn.answer_text,
            turn.contexts,
            request.history,
        );
        let generated = request.client.generate(
            request.api_key,
            command_model(request.command),
            &system_prompt,
            &user_prompt,
            &LlmConversationState::empty(),
            &LlmPromptContext::empty(),
        );
        match generated {
            Ok(generated) => {
                self.log_schema_fallback_if_needed(
                    request.command,
                    request.provider,
                    &generated.schema_fallback_reasons,
                );
                generated
                    .feedback
                    .normalized(
                        request.constraints,
                        turn.answer_text,
                        turn.language,
                        turn.contexts,
                    )
                    .recommendations()
                    .clone()
            }
            Err(error) => {
                tracing::warn!(
                    "recommendation generation failed sessionId={} model={} strict={} reason={}",
                    java_null(request.command.session_id.as_deref()),
                    java_null(request.command.model.as_deref()),
                    strict_deduplication,
                    failure_message(&error)
                );
                empty_recommendations()
            }
        }
    }
}

pub(super) fn command_model(command: &FeedbackCommand) -> &str {
    command.model.as_deref().unwrap_or("")
}

/// 로그에 Java `String.format("%s", null)`처럼 `null`을 쓴다.
pub(super) fn java_null(value: Option<&str>) -> &str {
    value.unwrap_or("null")
}

#[cfg(test)]
mod tests {
    use super::*;
    use omypic_domain::session::RecommendationRecord;

    fn recommendations(filler: &str, adjective: &str, adverb: &str) -> Recommendations {
        Recommendations::new(
            RecommendationDetail::new(filler, "usage"),
            RecommendationDetail::new(adjective, "usage"),
            RecommendationDetail::new(adverb, "usage"),
        )
    }

    #[test]
    fn history_normalizes_and_dedupes_terms_in_order() {
        let context = LlmPromptContext::new(
            "",
            vec![],
            vec![
                RecommendationRecord::new(" Well ", "Vivid", ""),
                RecommendationRecord::new("well", "nice", "really"),
            ],
        );

        let history = RecommendationHistory::from_prompt_context(&context);

        assert_eq!(vec!["well"], history.filler_terms);
        assert_eq!(vec!["vivid", "nice"], history.adjective_terms);
        assert_eq!(vec!["really"], history.adverb_terms);
    }

    #[test]
    fn evaluation_counts_missing_before_duplicates() {
        let history = RecommendationHistory {
            filler_terms: vec!["well".to_string()],
            adjective_terms: vec!["vivid".to_string()],
            adverb_terms: vec![],
        };
        let recs = Recommendations::new(
            RecommendationDetail::new("WELL", ""),
            RecommendationDetail::new("", ""),
            RecommendationDetail::new("really", "u"),
        );

        let evaluation = RecommendationEvaluation::of(&recs, &history);

        assert_eq!(1, evaluation.missing_count);
        assert_eq!(1, evaluation.duplicate_count);
        assert!(evaluation.should_attempt_repair());
    }

    #[test]
    fn better_recommendation_prefers_fewer_missing_then_fewer_duplicates() {
        let eval = |missing_count, duplicate_count| RecommendationEvaluation {
            missing_count,
            duplicate_count,
        };

        assert!(eval(0, 3).is_better_than(&eval(1, 0)));
        assert!(eval(1, 0).is_better_than(&eval(1, 2)));
        assert!(!eval(1, 2).is_better_than(&eval(1, 2)));
    }

    #[test]
    fn completeness_fills_only_empty_slots_with_language_specific_usage() {
        let recs = recommendations("Well", "", "");
        let recs = Recommendations::new(
            recs.filler().clone(),
            RecommendationDetail::new("", ""),
            RecommendationDetail::new("", " "),
        );

        let (completed, applied) =
            ensure_recommendation_completeness(&recs, &FeedbackLanguage::of("ko"));

        assert!(applied);
        assert_eq!("Well", completed.filler().term());
        assert_eq!(
            "이번 턴에서 자연스러운 형용사를 확정하지 못했습니다. 다음 답변 문맥에서 재생성합니다.",
            completed.adjective().usage()
        );
        assert_eq!(
            "이번 턴에서 자연스러운 부사를 확정하지 못했습니다. 다음 답변 문맥에서 재생성합니다.",
            completed.adverb().usage()
        );

        let (_, applied) = ensure_recommendation_completeness(
            &recommendations("a", "b", "c"),
            &FeedbackLanguage::of("en"),
        );
        assert!(!applied);
    }
}
