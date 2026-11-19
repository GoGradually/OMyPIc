//! Java `FeedbackUseCaseTest`. Mockito mock 대신 `test_support` 가짜와 실제 `RulebookUseCase`,
//! `WrongNoteUseCase`를 쓴다. 룰북 검색은 색인 가짜의 호출 기록으로, 오답노트 기록은 최근 큐
//! 저장 횟수로 확인한다.

use std::collections::HashSet;
use std::sync::Arc;

use chrono::{TimeZone, Utc};
use omypic_domain::error::AppError;
use omypic_domain::feedback::{
    CorrectionDetail, Corrections, Feedback, RecommendationDetail, Recommendations,
};
use omypic_domain::question::QuestionGroup;
use omypic_domain::rulebook::{Rulebook, RulebookContext, RulebookId, RulebookScope};
use omypic_domain::session::{LlmConversationState, ModeType, SessionId, SessionState};
use omypic_domain::shared::text_utils::{java_is_blank, utf16_len};

use super::FeedbackUseCase;
use crate::feedback::model::FeedbackCommand;
use crate::feedback::port::LlmGenerateResult;
use crate::rulebook::usecase::RulebookUseCase;
use crate::session::port::SharedSessionState;
use crate::test_support::{
    FakeFeedbackPolicy, FakeRagPolicy, InMemoryRulebookFileStore, InMemoryRulebookIndexPort,
    InMemoryRulebookPort, InMemorySessionStorePort, InMemoryWrongNotePort,
    InMemoryWrongNoteRecentQueuePort, RecordingMetrics, ScriptedLlmClient,
};
use crate::wrongnote::usecase::WrongNoteUseCase;

const RULEBOOK_ID: &str = "r1";
/// `FakeRagPolicy` 기본값: `search_contexts`가 색인에 요청하는 개수.
const SEARCH_TOP_K: usize = 3;

struct Fixture {
    open_ai: Arc<ScriptedLlmClient>,
    sessions: Arc<InMemorySessionStorePort>,
    index: Arc<InMemoryRulebookIndexPort>,
    metrics: Arc<RecordingMetrics>,
    wrong_note_queue: Arc<InMemoryWrongNoteRecentQueuePort>,
    use_case: FeedbackUseCase,
}

impl Fixture {
    fn new(open_ai: ScriptedLlmClient) -> Self {
        Self::with_extra_client(open_ai, None)
    }

    /// Java `setUp`과 `useCaseWith`. 질문 그룹 A 범위의 활성 룰북 `r1` 하나가 있다.
    fn with_extra_client(
        open_ai: ScriptedLlmClient,
        extra: Option<Arc<ScriptedLlmClient>>,
    ) -> Self {
        open_ai.set_bootstrap_responder(|call| {
            let previous = &call.conversation_state;
            let conversation_id = if previous.conversation_id().is_empty() {
                "conv-1"
            } else {
                previous.conversation_id()
            };
            Ok(LlmConversationState::new(
                conversation_id,
                "boot-resp",
                previous.turn_count_since_rebase(),
            ))
        });
        let open_ai = Arc::new(open_ai);
        let sessions = Arc::new(InMemorySessionStorePort::new());
        let index = Arc::new(InMemoryRulebookIndexPort::new());
        let metrics = Arc::new(RecordingMetrics::new());
        let wrong_note_queue = Arc::new(InMemoryWrongNoteRecentQueuePort::new());
        let policy = Arc::new(FakeFeedbackPolicy::default());
        let rulebook_use_case = Arc::new(RulebookUseCase::new(
            Arc::new(InMemoryRulebookPort::with_rulebooks(vec![
                group_a_rulebook(),
            ])),
            index.clone(),
            Arc::new(InMemoryRulebookFileStore::new()),
            Arc::new(FakeRagPolicy::default()),
            metrics.clone(),
        ));
        let wrong_note_use_case = Arc::new(WrongNoteUseCase::new(
            Arc::new(InMemoryWrongNotePort::new()),
            wrong_note_queue.clone(),
            policy.clone(),
        ));
        let mut clients: Vec<Arc<dyn crate::feedback::port::LlmClient>> = vec![open_ai.clone()];
        if let Some(extra) = extra {
            clients.push(extra);
        }
        let use_case = FeedbackUseCase::new(
            clients,
            rulebook_use_case,
            policy,
            metrics.clone(),
            sessions.clone(),
            wrong_note_use_case,
        )
        .unwrap();
        Self {
            open_ai,
            sessions,
            index,
            metrics,
            wrong_note_queue,
            use_case,
        }
    }

    fn session(&self, id: &str) -> SharedSessionState {
        self.sessions
            .insert(SessionState::new(SessionId::of(id).unwrap()))
    }

    fn session_with(&self, id: &str, setup: impl FnOnce(&mut SessionState)) -> SharedSessionState {
        let mut state = SessionState::new(SessionId::of(id).unwrap());
        setup(&mut state);
        self.sessions.insert(state)
    }

    /// `rulebookUseCase.searchContexts(query)`가 `contexts`를 돌려준다.
    fn stub_search_contexts(&self, contexts: Vec<RulebookContext>) {
        self.index
            .stub_search(SEARCH_TOP_K, rulebook_ids(), contexts);
    }

    fn search_queries(&self) -> Vec<String> {
        self.index
            .search_calls()
            .into_iter()
            .map(|call| call.query)
            .collect()
    }

    /// `wrongNoteUseCase.addFeedback` 호출 수 (호출마다 최근 큐를 한 번 저장한다).
    fn add_feedback_calls(&self) -> usize {
        self.wrong_note_queue.save_calls()
    }

    fn metric(&self, name: &str) -> usize {
        self.metrics.count(name)
    }
}

fn group_a_rulebook() -> Rulebook {
    let at = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    Rulebook::rehydrate(
        RulebookId::of(RULEBOOK_ID).unwrap(),
        "a.md",
        "/tmp/a.md",
        Some(RulebookScope::Question),
        Some(QuestionGroup::of("A").unwrap()),
        true,
        Some(at),
        Some(at),
    )
    .unwrap()
}

fn rulebook_ids() -> HashSet<RulebookId> {
    HashSet::from([RulebookId::of(RULEBOOK_ID).unwrap()])
}

fn context(filename: &str, text: &str) -> RulebookContext {
    RulebookContext::of(
        RulebookId::of(RULEBOOK_ID).unwrap(),
        Some(filename),
        Some(text),
    )
}

fn command(
    session_id: &str,
    provider: &str,
    language: Option<&str>,
    text: &str,
) -> FeedbackCommand {
    FeedbackCommand {
        session_id: Some(session_id.to_string()),
        text: Some(text.to_string()),
        provider: Some(provider.to_string()),
        model: Some("model".to_string()),
        feedback_language: language.map(str::to_string),
    }
}

fn corrections() -> Corrections {
    Corrections::new(
        CorrectionDetail::new("tense", "use past tense"),
        CorrectionDetail::new("wording", "use specific vocabulary"),
        CorrectionDetail::new("reason", "add one reason"),
    )
}

fn result_with_recommendations(
    filler: RecommendationDetail,
    adjective: RecommendationDetail,
    adverb: RecommendationDetail,
) -> LlmGenerateResult {
    LlmGenerateResult::new(
        Some(Feedback::of(
            "summary",
            corrections(),
            Recommendations::new(filler, adjective, adverb),
            "tiny",
            &[],
        )),
        Some(vec![]),
        Some(LlmConversationState::new("conv-1", "resp-1", 1)),
        Some("summary"),
    )
}

fn success_result() -> LlmGenerateResult {
    result_with_recommendations(
        RecommendationDetail::new("Well", "Use this to start naturally."),
        RecommendationDetail::new("impressive", "Use it for vivid detail."),
        RecommendationDetail::new("definitely", "Use it for confidence."),
    )
}

fn stateless_result() -> LlmGenerateResult {
    LlmGenerateResult::new(
        Some(success_result().feedback),
        Some(vec![]),
        Some(LlmConversationState::empty()),
        Some("summary"),
    )
}

fn open_ai(script: Vec<Result<LlmGenerateResult, AppError>>) -> ScriptedLlmClient {
    ScriptedLlmClient::new("openai", false, script)
}

fn stateless_client(script: Vec<Result<LlmGenerateResult, AppError>>) -> Arc<ScriptedLlmClient> {
    Arc::new(ScriptedLlmClient::new("anthropic", true, script))
}

fn failure(message: &str) -> Result<LlmGenerateResult, AppError> {
    Err(AppError::InvalidState(message.to_string()))
}

#[test]
fn generate_feedback_normalizes_required_fields_and_records_metrics() {
    let f = Fixture::new(open_ai(vec![Ok(success_result())]));
    let state = f.session("s2");
    f.stub_search_contexts(vec![context("rulebook.md", "always include evidence")]);

    let result = f
        .use_case
        .generate_feedback(
            "key",
            &command("s2", "OpenAI", Some("en"), "This is my answer."),
        )
        .unwrap();

    assert!(result.generated);
    let feedback = result.feedback.unwrap();
    assert!(!java_is_blank(feedback.corrections().grammar().issue()));
    assert!(!java_is_blank(feedback.recommendations().filler().term()));
    assert_eq!(1, feedback.rulebook_evidence().len());
    assert!(feedback.rulebook_evidence()[0].starts_with("[rulebook.md]"));
    assert!(utf16_len(feedback.example_answer()) >= 14);
    assert!(utf16_len(feedback.example_answer()) <= 21);
    assert_eq!("en", state.lock().unwrap().feedback_language().value());
    assert_eq!(vec!["This is my answer."], f.search_queries());
    assert_eq!(1, f.metric("record_feedback_latency"));
    assert_eq!(0, f.metric("increment_feedback_schema_fallback"));
    assert_eq!(0, f.metric("increment_feedback_error"));
    assert_eq!(1, f.add_feedback_calls());
}

#[test]
fn generate_feedback_in_continuous_mode_uses_most_recent_batch_text() {
    let f = Fixture::new(open_ai(vec![Ok(success_result())]));
    let state = f.session_with("s-batch", |s| {
        s.apply_mode_update(Some(ModeType::Continuous), Some(3))
    });

    let mut results = Vec::new();
    for text in ["first answer", "second answer", "third answer"] {
        state.lock().unwrap().append_segment(text);
        results.push(
            f.use_case
                .generate_feedback("key", &command("s-batch", "openai", Some("en"), text))
                .unwrap(),
        );
    }

    assert!(!results[0].generated);
    assert!(!results[1].generated);
    assert!(results[2].generated);
    assert_eq!(
        vec!["first answer\nsecond answer\nthird answer"],
        f.search_queries()
    );
}

#[test]
fn generate_feedback_uses_default_korean_language_when_command_language_is_null() {
    let f = Fixture::new(open_ai(vec![Ok(success_result())]));
    let state = f.session("s3");

    let result = f
        .use_case
        .generate_feedback("key", &command("s3", "openai", None, "짧은 답변"))
        .unwrap();

    assert!(result.generated);
    assert_eq!("ko", state.lock().unwrap().feedback_language().value());
}

#[test]
fn generate_feedback_clears_evidence_when_no_rulebook_context() {
    let all_issue = Corrections::new(
        CorrectionDetail::new("issue", "fix"),
        CorrectionDetail::new("issue", "fix"),
        CorrectionDetail::new("issue", "fix"),
    );
    let generated = LlmGenerateResult::new(
        Some(Feedback::of(
            "s",
            all_issue,
            Recommendations::new(
                RecommendationDetail::new("Well", "usage"),
                RecommendationDetail::new("vivid", "usage"),
                RecommendationDetail::new("definitely", "usage"),
            ),
            "123456789",
            &["should be removed".to_string()],
        )),
        Some(vec![]),
        Some(LlmConversationState::empty()),
        Some("s"),
    );
    let f = Fixture::new(open_ai(vec![Ok(generated)]));
    f.session("s4");

    let result = f
        .use_case
        .generate_feedback("key", &command("s4", "openai", Some("ko"), "123456789"))
        .unwrap();

    assert!(result.feedback.unwrap().rulebook_evidence().is_empty());
}

#[test]
fn generate_feedback_increments_schema_fallback_metric_when_client_reports_fallback() {
    let generated = LlmGenerateResult::new(
        Some(success_result().feedback),
        Some(vec!["structured_output_conversion_failed".to_string()]),
        Some(LlmConversationState::empty()),
        Some("summary"),
    );
    let f = Fixture::new(open_ai(vec![Ok(generated)]));
    f.session("s4-fallback");

    let result = f
        .use_case
        .generate_feedback(
            "key",
            &command("s4-fallback", "openai", Some("en"), "answer text"),
        )
        .unwrap();

    assert!(result.generated);
    assert!(f.metric("increment_feedback_schema_fallback") >= 1);
}

#[test]
fn generate_feedback_system_prompt_enforces_structured_contract() {
    let f = Fixture::new(open_ai(vec![Ok(success_result())]));
    f.session("s4-prompt");

    let result = f
        .use_case
        .generate_feedback(
            "key",
            &command("s4-prompt", "openai", Some("en"), "answer text"),
        )
        .unwrap();

    assert!(result.generated);
    let calls = f.open_ai.generate_calls();
    assert!(!calls.is_empty());
    let first_system_prompt = &calls[0].system_prompt;
    assert!(first_system_prompt
        .contains("summary, corrections, recommendations, exampleAnswer, rulebookEvidence"));
    assert!(
        first_system_prompt.contains("\"grammar\": {\"issue\": \"string\", \"fix\": \"string\"}")
    );
    assert!(
        first_system_prompt.contains("\"filler\": {\"term\": \"string\", \"usage\": \"string\"}")
    );
    assert!(first_system_prompt.contains("recommendations.term/usage는 \"추가 제안\" 전용이다"));
    assert!(first_system_prompt
        .contains("기존 표현을 제거하라는 피드백은 recommendation에 절대 포함하지 마라"));
    assert!(calls
        .iter()
        .any(|call| call.system_prompt.contains("중요 규칙(절대 준수):")));
    assert!(calls.iter().any(|call| call
        .system_prompt
        .contains("금지 표현 예시: \"remove\", \"eliminate\"")));
    assert!(calls
        .iter()
        .any(|call| call.user_prompt.contains("출력 점검 체크리스트:")));
    assert!(calls.iter().any(|call| call
        .user_prompt
        .contains("usage가 \"문장에 어떻게 덧붙일지\"를 설명하는가?")));
}

#[test]
fn generate_feedback_throws_and_increments_error_when_client_fails() {
    let f = Fixture::new(open_ai(vec![failure("bad response")]));
    f.session("s5");

    let error = f
        .use_case
        .generate_feedback("key", &command("s5", "openai", Some("ko"), "답변"))
        .unwrap_err();

    assert_eq!(
        AppError::InvalidState("LLM feedback failed: bad response".to_string()),
        error
    );
    assert_eq!(1, f.metric("increment_feedback_error"));
    assert_eq!(0, f.add_feedback_calls());
}

#[test]
fn generate_feedback_retries_once_when_conversation_is_invalid() {
    let f = Fixture::new(open_ai(vec![
        failure("conversation not found"),
        Ok(success_result()),
    ]));
    f.session_with("s5-retry", |s| {
        s.update_conversation_state(&LlmConversationState::new("conv-old", "resp-old", 2));
        s.mark_llm_bootstrapped();
    });

    let result = f
        .use_case
        .generate_feedback("key", &command("s5-retry", "openai", Some("en"), "answer"))
        .unwrap();

    assert!(result.generated);
    assert_eq!(3, f.open_ai.generate_calls().len());
    assert_eq!(1, f.open_ai.bootstrap_calls().len());
    assert_eq!(0, f.metric("increment_feedback_error"));
}

#[test]
fn generate_feedback_rebases_conversation_after_configured_turn_window() {
    let client = ScriptedLlmClient::with_responder("openai", false, |call| {
        let previous = &call.conversation_state;
        let conversation_id = if previous.conversation_id().is_empty() {
            "conv-1"
        } else {
            previous.conversation_id()
        };
        let next_turn_count = previous.turn_count_since_rebase() + 1;
        let feedback = Feedback::of(
            "summary",
            corrections(),
            Recommendations::new(
                RecommendationDetail::new(
                    &format!("Well-{next_turn_count}"),
                    "Use this to start naturally.",
                ),
                RecommendationDetail::new(
                    &format!("impressive-{next_turn_count}"),
                    "Use it for vivid detail.",
                ),
                RecommendationDetail::new(
                    &format!("definitely-{next_turn_count}"),
                    "Use it for confidence.",
                ),
            ),
            "tiny",
            &[],
        );
        Ok(LlmGenerateResult::new(
            Some(feedback),
            Some(vec![]),
            Some(LlmConversationState::new(
                conversation_id,
                &format!("resp-{next_turn_count}"),
                next_turn_count,
            )),
            Some("summary"),
        ))
    });
    let f = Fixture::new(client);
    f.session("s-rebase");

    for i in 0..7 {
        let result = f
            .use_case
            .generate_feedback(
                "key",
                &command("s-rebase", "openai", Some("en"), &format!("answer-{i}")),
            )
            .unwrap();
        assert!(result.generated);
    }

    let values: Vec<LlmConversationState> = f
        .open_ai
        .generate_calls()
        .into_iter()
        .map(|call| call.conversation_state)
        .collect();
    assert_eq!(14, values.len());
    assert!(f.open_ai.bootstrap_calls().is_empty());
    assert_eq!("", values[0].conversation_id());
    assert_eq!("", values[1].conversation_id());
    assert_eq!("conv-1", values[2].conversation_id());
    assert_eq!("conv-1", values[10].conversation_id());
    assert_eq!("", values[11].conversation_id());
    assert_eq!("", values[12].conversation_id());
    assert_eq!("", values[13].conversation_id());
}

#[test]
fn bootstrap_conversation_marks_session_as_bootstrapped() {
    let f = Fixture::new(open_ai(vec![Ok(success_result())]));
    let state = f.session("s-bootstrap");

    f.use_case
        .bootstrap_conversation(
            "key",
            &command("s-bootstrap", "openai", Some("en"), ""),
            Some("en"),
        )
        .unwrap();

    assert!(state.lock().unwrap().is_llm_bootstrapped());
    assert_eq!(1, f.open_ai.bootstrap_calls().len());
}

#[test]
fn generate_feedback_throws_for_unknown_provider_without_incrementing_error_metric() {
    let f = Fixture::new(open_ai(vec![Ok(success_result())]));
    f.session("s6");

    let error = f
        .use_case
        .generate_feedback("key", &command("s6", "claude", Some("ko"), "답변"))
        .unwrap_err();

    assert!(matches!(error, AppError::InvalidArgument(_)), "{error:?}");
    assert_eq!(0, f.metric("increment_feedback_error"));
}

#[test]
fn generate_feedback_for_turn_uses_question_group_scoped_contexts() {
    let f = Fixture::new(open_ai(vec![Ok(success_result())]));
    f.session("s-turn");
    // 질문 그룹 A 범위 룰북 하나: search_contexts_for_turn(A, query, 2)가 색인에 (query, 2, {r1})로 묻는다.
    f.index
        .stub_search(2, rulebook_ids(), vec![context("a.md", "group A rules")]);

    let feedback = f
        .use_case
        .generate_feedback_for_turn(
            "key",
            &command("s-turn", "openai", Some("en"), "ignored"),
            "Question text",
            Some(&QuestionGroup::of("A").unwrap()),
            "Answer text",
            2,
        )
        .unwrap();

    assert_eq!(1, feedback.rulebook_evidence().len());
    assert!(feedback.rulebook_evidence()[0].starts_with("[a.md]"));
    let queries = f.search_queries();
    assert!(!queries.is_empty());
    assert!(queries.iter().all(|q| q == "Question text\nAnswer text"));
    assert_eq!(1, f.add_feedback_calls());
}

#[test]
fn generate_feedback_for_turn_with_prefetch_reuses_prefetched_prompt_without_extra_search() {
    let f = Fixture::new(open_ai(vec![Ok(success_result())]));
    f.session("s-turn");
    f.index
        .stub_search(2, rulebook_ids(), vec![context("a.md", "group A rules")]);

    let prefetch = f
        .use_case
        .prefetch_turn_prompt(
            "q-1",
            "Question text",
            Some(&QuestionGroup::of("A").unwrap()),
            Some("en"),
            2,
        )
        .unwrap();
    let searches_after_prefetch = f.index.search_calls().len();
    let feedback = f
        .use_case
        .generate_feedback_for_turn_with_prefetch(
            "key",
            &command("s-turn", "openai", Some("en"), "ignored"),
            "Answer text",
            Some(&prefetch),
        )
        .unwrap();

    assert_eq!(1, feedback.rulebook_evidence().len());
    assert!(searches_after_prefetch > 0);
    assert_eq!(searches_after_prefetch, f.index.search_calls().len());
    assert!(f.search_queries().iter().all(|q| q == "Question text"));
    assert!(f
        .open_ai
        .generate_calls()
        .iter()
        .any(|call| call.user_prompt.contains("Answer text")));
    assert_eq!(1, f.add_feedback_calls());
}

#[test]
fn generate_feedback_recommendation_repair_tracks_duplicate_when_it_cannot_improve() {
    let f = Fixture::new(open_ai(vec![
        Ok(success_result()),
        Ok(success_result()),
        Ok(success_result()),
    ]));
    f.session_with("s-duplicate", |s| {
        s.append_llm_recommendation_terms("well", "impressive", "definitely", 5)
    });

    let result = f
        .use_case
        .generate_feedback(
            "key",
            &command("s-duplicate", "openai", Some("en"), "answer"),
        )
        .unwrap();

    assert!(result.generated);
    assert_eq!(3, f.open_ai.generate_calls().len());
    assert_eq!(1, f.metric("increment_recommendation_repair_attempt"));
    assert_eq!(1, f.metric("increment_recommendation_duplicate_detected"));
    assert_eq!(0, f.metric("increment_recommendation_repair_success"));
    assert_eq!(0, f.metric("increment_recommendation_minimal_fallback"));
}

#[test]
fn generate_feedback_applies_minimal_recommendation_fallback_when_repair_fails() {
    let empty = || RecommendationDetail::new("", "");
    let f = Fixture::new(open_ai(vec![
        Ok(result_with_recommendations(empty(), empty(), empty())),
        failure("candidate failed"),
        failure("repair failed"),
    ]));
    f.session("s-minimal");

    let result = f
        .use_case
        .generate_feedback(
            "key",
            &command("s-minimal", "openai", Some("en"), "answer text"),
        )
        .unwrap();

    assert!(result.generated);
    let feedback = result.feedback.unwrap();
    let recommendations = feedback.recommendations();
    assert!(java_is_blank(recommendations.filler().term()));
    assert!(!java_is_blank(recommendations.filler().usage()));
    assert!(java_is_blank(recommendations.adjective().term()));
    assert!(!java_is_blank(recommendations.adjective().usage()));
    assert!(java_is_blank(recommendations.adverb().term()));
    assert!(!java_is_blank(recommendations.adverb().usage()));
    assert_eq!(3, f.open_ai.generate_calls().len());
    assert_eq!(1, f.metric("increment_recommendation_repair_attempt"));
    assert_eq!(1, f.metric("increment_recommendation_minimal_fallback"));
}

#[test]
fn bootstrap_conversation_skips_stateless_provider() {
    let claude = stateless_client(vec![Ok(stateless_result())]);
    let f = Fixture::with_extra_client(open_ai(vec![Ok(success_result())]), Some(claude.clone()));
    let state = f.session("s-claude-boot");

    f.use_case
        .bootstrap_conversation(
            "sk-ant-key",
            &command("s-claude-boot", "anthropic", Some("en"), ""),
            Some("en"),
        )
        .unwrap();

    assert!(!state.lock().unwrap().is_llm_bootstrapped());
    assert!(claude.bootstrap_calls().is_empty());
    assert!(f.open_ai.bootstrap_calls().is_empty());
}

#[test]
fn generate_feedback_stateless_provider_always_sends_system_prompt_and_keeps_stored_conversation() {
    let claude = stateless_client(vec![Ok(stateless_result())]);
    let f = Fixture::with_extra_client(open_ai(vec![Ok(success_result())]), Some(claude.clone()));
    let state = f.session_with("s-claude", |s| {
        s.update_conversation_state(&LlmConversationState::new("conv-openai", "resp-openai", 2));
        s.mark_llm_bootstrapped();
    });

    let result = f
        .use_case
        .generate_feedback(
            "sk-ant-key",
            &command("s-claude", "anthropic", Some("en"), "This is my answer."),
        )
        .unwrap();

    assert!(result.generated);
    let calls = claude.generate_calls();
    assert!(!calls.is_empty());
    assert!(calls.iter().all(|call| call.api_key == "sk-ant-key"));
    assert!(!java_is_blank(&calls[0].system_prompt));
    assert!(!calls[0].conversation_state.has_conversation_id());
    assert_eq!(
        "conv-openai",
        state.lock().unwrap().conversation_state().conversation_id()
    );
    assert!(f.open_ai.generate_calls().is_empty());
}

// Java 테스트에 없는 Rust 쪽 보강.

#[test]
fn new_rejects_duplicate_provider_clients() {
    let first: Arc<dyn crate::feedback::port::LlmClient> =
        Arc::new(open_ai(vec![Ok(success_result())]));
    let second: Arc<dyn crate::feedback::port::LlmClient> =
        Arc::new(open_ai(vec![Ok(success_result())]));
    let f = Fixture::new(open_ai(vec![Ok(success_result())]));
    let policy = Arc::new(FakeFeedbackPolicy::default());
    let rulebooks = Arc::new(RulebookUseCase::new(
        Arc::new(InMemoryRulebookPort::new()),
        f.index.clone(),
        Arc::new(InMemoryRulebookFileStore::new()),
        Arc::new(FakeRagPolicy::default()),
        f.metrics.clone(),
    ));
    let notes = Arc::new(WrongNoteUseCase::new(
        Arc::new(InMemoryWrongNotePort::new()),
        f.wrong_note_queue.clone(),
        policy.clone(),
    ));

    let result = FeedbackUseCase::new(
        vec![first, second],
        rulebooks,
        policy,
        f.metrics.clone(),
        f.sessions.clone(),
        notes,
    );

    assert_eq!(
        Some(AppError::InvalidState("Duplicate key openai".to_string())),
        result.err()
    );
}

#[test]
fn bootstrap_conversation_requires_session_id() {
    let f = Fixture::new(open_ai(vec![Ok(success_result())]));

    let error = f
        .use_case
        .bootstrap_conversation("key", &command(" ", "openai", Some("en"), ""), Some("en"))
        .unwrap_err();

    assert_eq!(
        AppError::InvalidArgument("sessionId is required".to_string()),
        error
    );
}

#[test]
fn generate_feedback_for_turn_with_prefetch_requires_prefetch() {
    let f = Fixture::new(open_ai(vec![Ok(success_result())]));

    let error = f
        .use_case
        .generate_feedback_for_turn_with_prefetch(
            "key",
            &command("s", "openai", Some("en"), ""),
            "answer",
            None,
        )
        .unwrap_err();

    assert_eq!(
        AppError::InvalidArgument("prefetch is required".to_string()),
        error
    );
}

#[test]
fn generate_feedback_for_turn_with_prefetch_rebuilds_system_prompt_when_language_changes() {
    let f = Fixture::new(open_ai(vec![Ok(success_result())]));
    f.session("s-lang");
    let prefetch = f
        .use_case
        .prefetch_turn_prompt(" q-1 ", " Question text ", None, Some("ko"), 2)
        .unwrap();
    assert_eq!("q-1", prefetch.question_id);
    assert_eq!("Question text", prefetch.question_text);
    assert_eq!("ko", prefetch.language);
    assert!(prefetch.system_prompt.ends_with("Korean로 작성하라.\n"));

    f.use_case
        .generate_feedback_for_turn_with_prefetch(
            "key",
            &command("s-lang", "openai", Some("en"), ""),
            "Answer text",
            Some(&prefetch),
        )
        .unwrap();

    let calls = f.open_ai.generate_calls();
    assert!(calls[0].system_prompt.ends_with("English로 작성하라.\n"));
    assert!(calls[0]
        .user_prompt
        .starts_with("# 질문 별 프롬프트\n\n다음은 너가 읽어줘야 할 질문이다.\n\nQuestion text\n"));
}

#[test]
fn generate_feedback_records_turn_and_recommendation_history_in_session() {
    let f = Fixture::new(open_ai(vec![Ok(success_result())]));
    let state = f.session("s-history");

    f.use_case
        .generate_feedback("key", &command("s-history", "openai", Some("en"), "answer"))
        .unwrap();

    let session = state.lock().unwrap();
    let context = session.build_prompt_context();
    assert_eq!("summary", context.summary());
    assert_eq!(1, context.recent_turns().len());
    assert_eq!("answer", context.recent_turns()[0].answer());
    assert_eq!("Well", context.recent_recommendations()[0].filler_term());
    assert_eq!("conv-1", session.conversation_state().conversation_id());
}
