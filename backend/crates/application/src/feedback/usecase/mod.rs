//! Java `FeedbackUseCase`: 프롬프트를 만들어 LLM에 피드백을 요청하고, 정규화한 뒤 추천 표현을
//! 보강하고, 세션의 대화 상태와 오답노트를 갱신한다.
//!
//! - `prompts`: 시스템/사용자 프롬프트 (템플릿은 `templates`)
//! - `conversation`: 서버 대화(OpenAI Responses)의 rebase, 잘못된 대화 복구, bootstrap
//! - `recommendation`: 추천 표현 평가, repair, 최소 대체값
//!
//! 세션 상태는 짧게만 잠근다. LLM 호출 중에는 잠그지 않으므로 호출자도 세션 잠금을 쥔 채로
//! 이 유스케이스를 부르면 안 된다.

mod conversation;
mod prompts;
mod recommendation;
mod templates;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use omypic_domain::error::{AppError, AppResult};
use omypic_domain::feedback::{Feedback, FeedbackConstraints, FeedbackLanguage};
use omypic_domain::question::QuestionGroup;
use omypic_domain::rulebook::RulebookContext;
use omypic_domain::session::SessionId;
use omypic_domain::shared::text_utils::java_is_blank;

use self::prompts::{
    build_system_prompt, build_turn_prompt, build_turn_query, build_user_prompt, trim_text,
    PromptInput,
};
use self::recommendation::{java_null, RecommendationHistory, RecommendationRequest};
use crate::feedback::model::{FeedbackCommand, FeedbackResult};
use crate::feedback::policy::{FeedbackModelPolicy, FeedbackPolicy};
use crate::feedback::port::{LlmClient, LlmGenerateResult};
use crate::rulebook::usecase::RulebookUseCase;
use crate::session::port::{lock_session, SessionStorePort, SharedSessionState};
use crate::shared::port::MetricsPort;
use crate::wrongnote::usecase::WrongNoteUseCase;

const DEFAULT_CONVERSATION_REBASE_TURNS: i32 = 6;
const RECENT_TURNS_LIMIT: usize = 6;
const RECENT_RECOMMENDATIONS_LIMIT: usize = 5;

/// 다음 질문의 룰북 검색과 시스템 프롬프트를 미리 만들어 둔 것 (Java `FeedbackUseCase.PrefetchedTurnPrompt`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrefetchedTurnPrompt {
    pub question_id: String,
    pub question_text: String,
    pub contexts: Vec<RulebookContext>,
    pub language: String,
    pub system_prompt: String,
}

/// 피드백 요청 하나의 입력. 프롬프트는 이미 만들어져 있다.
pub(super) struct TurnRequest<'a> {
    pub(super) language: &'a FeedbackLanguage,
    pub(super) contexts: &'a [RulebookContext],
    pub(super) question_text: &'a str,
    /// 정규화(예시 답변 길이)와 추천 요청에 쓰는 사용자 답변.
    pub(super) answer_text: &'a str,
    pub(super) system_prompt: &'a str,
    pub(super) user_prompt: &'a str,
}

pub struct FeedbackUseCase {
    clients: HashMap<String, Arc<dyn LlmClient>>,
    rulebook_use_case: Arc<RulebookUseCase>,
    feedback_policy: Arc<dyn FeedbackPolicy>,
    metrics: Arc<dyn MetricsPort>,
    session_store: Arc<dyn SessionStorePort>,
    wrong_note_use_case: Arc<WrongNoteUseCase>,
    conversation_rebase_turns: i32,
}

impl FeedbackUseCase {
    /// 대화 rebase 주기는 6턴이다. 제공자가 같은 클라이언트가 둘 있으면 Java `Collectors.toMap`처럼
    /// `InvalidState("Duplicate key <provider>")`다.
    pub fn new(
        clients: Vec<Arc<dyn LlmClient>>,
        rulebook_use_case: Arc<RulebookUseCase>,
        feedback_policy: Arc<dyn FeedbackPolicy>,
        metrics: Arc<dyn MetricsPort>,
        session_store: Arc<dyn SessionStorePort>,
        wrong_note_use_case: Arc<WrongNoteUseCase>,
    ) -> AppResult<Self> {
        Self::with_conversation_rebase_turns(
            clients,
            rulebook_use_case,
            feedback_policy,
            metrics,
            session_store,
            wrong_note_use_case,
            DEFAULT_CONVERSATION_REBASE_TURNS,
        )
    }

    /// `conversation_rebase_turns`는 1보다 작으면 1이다.
    pub fn with_conversation_rebase_turns(
        clients: Vec<Arc<dyn LlmClient>>,
        rulebook_use_case: Arc<RulebookUseCase>,
        feedback_policy: Arc<dyn FeedbackPolicy>,
        metrics: Arc<dyn MetricsPort>,
        session_store: Arc<dyn SessionStorePort>,
        wrong_note_use_case: Arc<WrongNoteUseCase>,
        conversation_rebase_turns: i32,
    ) -> AppResult<Self> {
        let mut by_provider = HashMap::new();
        for client in clients {
            let provider = client.provider();
            if by_provider.contains_key(&provider) {
                return Err(AppError::InvalidState(format!("Duplicate key {provider}")));
            }
            by_provider.insert(provider, client);
        }
        Ok(Self {
            clients: by_provider,
            rulebook_use_case,
            feedback_policy,
            metrics,
            session_store,
            wrong_note_use_case,
            conversation_rebase_turns: conversation_rebase_turns.max(1),
        })
    }

    /// 텍스트 하나에 대한 피드백. 연속 모드에서 묶음이 덜 찼으면 건너뛰고, 찼으면 모인 STT 조각을
    /// 합쳐서 쓴다.
    pub fn generate_feedback(
        &self,
        api_key: &str,
        command: &FeedbackCommand,
    ) -> AppResult<FeedbackResult> {
        let state = self.session_state(command)?;
        let language = feedback_language(command.feedback_language.as_deref());
        let input_text = {
            let mut session = lock_session(&state)?;
            session.set_feedback_language(language.clone());
            if !session.should_generate_feedback() {
                return Ok(FeedbackResult::skipped());
            }
            session.resolve_feedback_input_text(command.text.as_deref().unwrap_or(""))
        };
        let prompt_input = PromptInput::from_general(&input_text);
        let contexts = self.rulebook_use_case.search_contexts(&input_text)?;
        let feedback = self.generate_feedback_internal(
            api_key,
            command,
            &state,
            &language,
            &prompt_input,
            contexts,
        )?;
        Ok(FeedbackResult::generated(feedback))
    }

    /// 질문 하나와 그 답변에 대한 피드백. 룰북은 질문 그룹 범위로 최대 `max_rulebook_documents`개 찾는다.
    pub fn generate_feedback_for_turn(
        &self,
        api_key: &str,
        command: &FeedbackCommand,
        question_text: &str,
        question_group: Option<&QuestionGroup>,
        answer_text: &str,
        max_rulebook_documents: usize,
    ) -> AppResult<Feedback> {
        let language = feedback_language(command.feedback_language.as_deref());
        let state = self.session_state(command)?;
        let query = build_turn_query(question_text, answer_text);
        let prompt_input = PromptInput::from_turn(question_text, answer_text);
        let contexts = self.rulebook_use_case.search_contexts_for_turn(
            question_group,
            &query,
            max_rulebook_documents,
        )?;
        self.generate_feedback_internal(
            api_key,
            command,
            &state,
            &language,
            &prompt_input,
            contexts,
        )
    }

    /// 답변이 오기 전에 질문만으로 룰북을 찾고 시스템 프롬프트를 만들어 둔다.
    pub fn prefetch_turn_prompt(
        &self,
        question_id: &str,
        question_text: &str,
        question_group: Option<&QuestionGroup>,
        feedback_language: Option<&str>,
        max_rulebook_documents: usize,
    ) -> AppResult<PrefetchedTurnPrompt> {
        let language = self::feedback_language(feedback_language);
        let safe_question_text = trim_text(question_text);
        let query = build_turn_query(&safe_question_text, "");
        let contexts = self.rulebook_use_case.search_contexts_for_turn(
            question_group,
            &query,
            max_rulebook_documents,
        )?;
        let system_prompt = build_system_prompt(language.value(), &contexts);
        Ok(PrefetchedTurnPrompt {
            question_id: trim_text(question_id),
            question_text: safe_question_text,
            contexts,
            language: language.value().to_string(),
            system_prompt,
        })
    }

    /// `prefetch_turn_prompt`의 결과로 룰북을 다시 찾지 않고 피드백을 만든다. 언어가 바뀌었으면
    /// 시스템 프롬프트만 다시 만든다.
    pub fn generate_feedback_for_turn_with_prefetch(
        &self,
        api_key: &str,
        command: &FeedbackCommand,
        answer_text: &str,
        prefetch: Option<&PrefetchedTurnPrompt>,
    ) -> AppResult<Feedback> {
        let prefetch = prefetch
            .ok_or_else(|| AppError::InvalidArgument("prefetch is required".to_string()))?;
        let language = feedback_language(command.feedback_language.as_deref());
        let state = self.session_state(command)?;
        let contexts = prefetch.contexts.as_slice();
        let safe_answer = trim_text(answer_text);
        let system_prompt = resolve_system_prompt(prefetch, language.value(), contexts);
        let user_prompt = build_turn_prompt(&prefetch.question_text, &safe_answer, contexts);
        self.generate_feedback_from_prompts(
            api_key,
            command,
            &state,
            &TurnRequest {
                language: &language,
                contexts,
                question_text: &prefetch.question_text,
                answer_text: &safe_answer,
                system_prompt: &system_prompt,
                user_prompt: &user_prompt,
            },
        )
    }

    fn generate_feedback_internal(
        &self,
        api_key: &str,
        command: &FeedbackCommand,
        state: &SharedSessionState,
        language: &FeedbackLanguage,
        prompt_input: &PromptInput,
        contexts: Vec<RulebookContext>,
    ) -> AppResult<Feedback> {
        let system_prompt = build_system_prompt(language.value(), &contexts);
        let user_prompt = build_user_prompt(prompt_input, &contexts, language.value());
        self.generate_feedback_from_prompts(
            api_key,
            command,
            state,
            &TurnRequest {
                language,
                contexts: &contexts,
                question_text: &prompt_input.question_text,
                answer_text: prompt_input.normalization_text(),
                system_prompt: &system_prompt,
                user_prompt: &user_prompt,
            },
        )
    }

    /// 제공자와 클라이언트를 고른 뒤 요청한다. 그 뒤의 실패는 `feedback_error`를 세고
    /// `InvalidState("LLM feedback failed: <원인>")`가 된다.
    fn generate_feedback_from_prompts(
        &self,
        api_key: &str,
        command: &FeedbackCommand,
        state: &SharedSessionState,
        turn: &TurnRequest<'_>,
    ) -> AppResult<Feedback> {
        let provider = FeedbackModelPolicy::normalize_provider(command.provider.as_deref())?;
        let client = self.resolve_client(provider)?;
        self.rebase_conversation_if_needed(client.as_ref(), state)?;
        let start = Instant::now();
        self.request_feedback(
            api_key,
            command,
            client.as_ref(),
            provider,
            state,
            turn,
            start,
        )
        .map_err(|error| {
            self.metrics.increment_feedback_error();
            AppError::InvalidState(format!("LLM feedback failed: {}", failure_message(&error)))
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn request_feedback(
        &self,
        api_key: &str,
        command: &FeedbackCommand,
        client: &dyn LlmClient,
        provider: &str,
        state: &SharedSessionState,
        turn: &TurnRequest<'_>,
        start: Instant,
    ) -> AppResult<Feedback> {
        let prompt_context = lock_session(state)?.build_prompt_context();
        let generated = self.generate_with_conversation_recovery(
            client,
            api_key,
            command,
            turn,
            state,
            &prompt_context,
        )?;
        self.log_schema_fallback_if_needed(command, provider, &generated.schema_fallback_reasons);
        let constraints = self.feedback_constraints();
        let normalized = generated.feedback.normalized(
            &constraints,
            turn.answer_text,
            turn.language,
            turn.contexts,
        );
        let history = RecommendationHistory::from_prompt_context(&prompt_context);
        let recommendations = self.select_recommendations(
            &RecommendationRequest {
                api_key,
                command,
                client,
                provider,
                constraints: &constraints,
                turn,
                history: &history,
            },
            normalized.recommendations(),
        );
        let feedback = Feedback::of(
            normalized.summary(),
            normalized.corrections().clone(),
            recommendations,
            normalized.example_answer(),
            normalized.rulebook_evidence(),
        );
        {
            let mut session = lock_session(state)?;
            if !client.is_stateless() {
                session.update_conversation_state(&generated.conversation_state);
            }
            session.set_llm_summary(resolve_summary_for_next_turn(&generated, &feedback));
            session.append_llm_turn(
                turn.question_text,
                turn.answer_text,
                feedback.summary(),
                RECENT_TURNS_LIMIT,
            );
            let recommendations = feedback.recommendations();
            session.append_llm_recommendation_terms(
                recommendations.filler().term(),
                recommendations.adjective().term(),
                recommendations.adverb().term(),
                RECENT_RECOMMENDATIONS_LIMIT,
            );
        }
        self.metrics.record_feedback_latency(start.elapsed());
        self.wrong_note_use_case.add_feedback(&feedback)?;
        Ok(feedback)
    }

    fn session_state(&self, command: &FeedbackCommand) -> AppResult<SharedSessionState> {
        let session_id = SessionId::of(command.session_id.as_deref().unwrap_or(""))?;
        self.session_store.get_or_create(&session_id)
    }

    fn feedback_constraints(&self) -> FeedbackConstraints {
        FeedbackConstraints::new(
            self.feedback_policy.get_summary_max_chars(),
            self.feedback_policy.get_example_min_ratio(),
            self.feedback_policy.get_example_max_ratio(),
        )
    }

    fn resolve_client(&self, provider: &str) -> AppResult<Arc<dyn LlmClient>> {
        self.clients.get(provider).cloned().ok_or_else(|| {
            AppError::InvalidState(format!(
                "LLM client is not configured for provider: {provider}"
            ))
        })
    }

    pub(super) fn log_schema_fallback_if_needed(
        &self,
        command: &FeedbackCommand,
        provider: &str,
        reasons: &[String],
    ) {
        if reasons.is_empty() {
            return;
        }
        self.metrics.increment_feedback_schema_fallback();
        tracing::warn!(
            "feedback schema fallback applied sessionId={} provider={} model={} reasons=[{}]",
            java_null(command.session_id.as_deref()),
            provider,
            java_null(command.model.as_deref()),
            reasons.join(", ")
        );
    }
}

fn feedback_language(value: Option<&str>) -> FeedbackLanguage {
    FeedbackLanguage::of(value.unwrap_or(""))
}

/// 미리 만든 시스템 프롬프트는 언어가 같을 때만 다시 쓴다.
fn resolve_system_prompt(
    prefetch: &PrefetchedTurnPrompt,
    language: &str,
    contexts: &[RulebookContext],
) -> String {
    if language.eq_ignore_ascii_case(&prefetch.language) {
        return prefetch.system_prompt.clone();
    }
    build_system_prompt(language, contexts)
}

fn resolve_summary_for_next_turn<'a>(
    generated: &'a LlmGenerateResult,
    feedback: &'a Feedback,
) -> &'a str {
    if !java_is_blank(&generated.summary_for_next_turn) {
        return &generated.summary_for_next_turn;
    }
    feedback.summary()
}

/// Java `failureMessage`: 오류 메시지이고, 비었으면 `unknown cause`다.
pub(super) fn failure_message(error: &AppError) -> String {
    let message = error.to_string();
    if java_is_blank(&message) {
        "unknown cause".to_string()
    } else {
        message
    }
}
