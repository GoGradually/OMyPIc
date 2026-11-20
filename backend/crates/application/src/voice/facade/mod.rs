//! Java `VoiceSessionFacade`: 음성 세션의 열기, 싱크, 복구, 오디오/답변 받기, 정지.
//!
//! - `context`: 세션 하나의 런타임 상태와 잠금
//! - `settings`: 세션을 열 때 정해지는 모델과 제공자
//! - `turn`: 턴 처리(STT, 피드백, 다음 질문)
//! - `events`: 이벤트 payload와 TTS 발행
//! - `prefetch`: 다음 질문의 룰북 검색과 프롬프트를 미리 만들기
//! - `audio`: base64와 WAV
//!
//! 비동기 작업은 `AsyncExecutor`로 돌린다. 작업이 facade를 잡고 있어야 하므로 facade는 `Arc`로
//! 쓴다. 어떤 잠금도 쥔 채로 유스케이스나 게이트웨이를 부르지 않는다.

mod audio;
mod context;
mod events;
mod prefetch;
mod settings;
mod turn;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use omypic_domain::error::{AppError, AppResult};
use omypic_domain::session::SessionStopReason;
use omypic_domain::shared::text_utils::java_is_blank;
use serde_json::json;
use uuid::Uuid;

use self::audio::{decode_base64, DEFAULT_SAMPLE_RATE};
use self::context::{lock, RuntimeContext};
use self::events::{error_message, error_payload, is_question_exhausted, question_payload};
use self::settings::{is_ai_enabled, RuntimeSettings};
use crate::feedback::model::FeedbackCommand;
use crate::feedback::policy::FeedbackModelPolicy;
use crate::session::port::lock_session;
use crate::voice::model::{
    VoiceAnswerCommand, VoiceAudioChunkCommand, VoiceEventSink, VoiceSessionOpenCommand,
    VoiceSessionStopCommand,
};
use crate::voice::usecase::{NowMillis, RecoverySnapshot, VoiceSessionDependencies};

const DEFAULT_RECOVERY_RETENTION_MS: i64 = 600_000;
const DEFAULT_STOPPED_CONTEXT_MAX: usize = 1000;
const DEFAULT_IDLE_TIMEOUT_MS: i64 = 600_000;
/// 이벤트 스트림 없이 오래 방치돼 서버가 멈춘 세션의 정지 이유.
const IDLE_TIMEOUT_STOP_REASON: &str = "idle_timeout";
const MAX_RULEBOOK_DOCUMENTS_PER_TURN: usize = 2;

pub(crate) struct VoiceSessionFacade {
    deps: VoiceSessionDependencies,
    event_replay_buffer_limit: usize,
    now_millis: NowMillis,
    contexts: Mutex<HashMap<String, Arc<RuntimeContext>>>,
}

impl VoiceSessionFacade {
    pub(crate) fn new(
        deps: VoiceSessionDependencies,
        event_replay_buffer_limit: usize,
        now_millis: NowMillis,
    ) -> Self {
        Self {
            deps,
            event_replay_buffer_limit: event_replay_buffer_limit.max(1),
            now_millis,
            contexts: Mutex::new(HashMap::new()),
        }
    }

    pub(crate) fn open(&self, command: &VoiceSessionOpenCommand) -> AppResult<String> {
        self.purge_contexts();
        self.validate_open_command(command)?;
        let session_id = command.session_id.clone().unwrap_or_default();
        self.deps.session_use_case.get_or_create(&session_id)?;
        let settings = RuntimeSettings::resolve(command, self.deps.voice_policy.as_ref())?;
        let feedback_api_key = if settings.uses_anthropic_feedback() {
            command.feedback_api_key.clone()
        } else {
            command.api_key.clone()
        };
        let context = Arc::new(RuntimeContext::new(
            Uuid::new_v4().to_string(),
            session_id,
            command.api_key.clone().unwrap_or_default(),
            feedback_api_key.unwrap_or_default(),
            settings,
            self.event_replay_buffer_limit,
            Arc::clone(&self.now_millis),
        ));
        let voice_session_id = context.voice_session_id.clone();
        lock(&self.contexts).insert(voice_session_id.clone(), context);
        Ok(voice_session_id)
    }

    fn validate_open_command(&self, command: &VoiceSessionOpenCommand) -> AppResult<()> {
        if is_blank(command.session_id.as_deref()) {
            return Err(invalid("sessionId is required"));
        }
        if !is_ai_enabled(command) {
            // AI가 꺼져 있으면 어떤 제공자도 부르지 않으므로 키도 모델도 필요 없다.
            return Ok(());
        }
        let provider = command
            .feedback_provider
            .as_deref()
            .filter(|provider| !java_is_blank(provider));
        FeedbackModelPolicy::validate_or_throw_for_provider(
            provider,
            command.feedback_model.as_deref(),
        )?;
        let settings = RuntimeSettings::resolve(command, self.deps.voice_policy.as_ref())?;
        if settings.needs_open_ai_key() && is_blank(command.api_key.as_deref()) {
            return Err(invalid("sessionId and apiKey are required"));
        }
        if settings.uses_anthropic_feedback() && is_blank(command.feedback_api_key.as_deref()) {
            return Err(invalid(
                "Anthropic API key is required for Claude feedback models",
            ));
        }
        Ok(())
    }

    /// 싱크를 등록한다. 처음 등록될 때 세션 초기화(bootstrap, 첫 질문)를 비동기로 시작한다.
    pub(crate) fn register_sink(
        self: &Arc<Self>,
        voice_session_id: &str,
        sink: Arc<dyn VoiceEventSink>,
        since_event_id: Option<i64>,
    ) -> AppResult<()> {
        self.purge_contexts();
        let context = self.require_context(voice_session_id)?;
        context.add_sink(sink, since_event_id);
        if !context.mark_initialized() {
            return Ok(());
        }
        let this = Arc::clone(self);
        self.deps
            .async_executor
            .execute(Box::new(move || this.initialize_session(&context)));
        Ok(())
    }

    pub(crate) fn unregister_sink(&self, voice_session_id: &str, sink: &Arc<dyn VoiceEventSink>) {
        self.purge_contexts();
        if let Some(context) = self.find_context(voice_session_id) {
            context.remove_sink(sink);
        }
    }

    pub(crate) fn recover(
        &self,
        voice_session_id: &str,
        last_seen_event_id: Option<i64>,
    ) -> AppResult<RecoverySnapshot> {
        self.purge_contexts();
        let context = self.require_context(voice_session_id)?;
        Ok(context.snapshot_for_recovery(last_seen_event_id.unwrap_or(0).max(0)))
    }

    pub(crate) fn append_audio(
        self: &Arc<Self>,
        command: &VoiceAudioChunkCommand,
    ) -> AppResult<()> {
        self.purge_contexts();
        if is_blank(command.voice_session_id.as_deref()) {
            return Err(invalid("voiceSessionId is required"));
        }
        let Some(pcm16_base64) = command
            .pcm16_base64
            .as_deref()
            .filter(|value| !java_is_blank(value))
        else {
            return Err(invalid("pcm16Base64 is required"));
        };
        let context = self.require_context(command.voice_session_id.as_deref().unwrap_or(""))?;
        if context.is_inactive() || !context.settings.server_stt() {
            // 서버에서 전사하는 세션만 오디오를 받는다. 나머지는 complete_answer로 답을 끝낸다.
            return Ok(());
        }
        if !context.accept_chunk_sequence(command.sequence) {
            return Ok(());
        }
        let pcm16 =
            decode_base64(pcm16_base64).ok_or_else(|| invalid("Invalid base64 audio chunk"))?;
        if pcm16.is_empty() {
            return Ok(());
        }
        context.append_audio(&pcm16, command.sample_rate.unwrap_or(DEFAULT_SAMPLE_RATE));
        self.flush_turn(&context);
        Ok(())
    }

    /// 학습자가 오디오 없이 답을 마쳤다고 알린다. AI가 꺼져 있으면 전사 없이 다음 질문으로
    /// 넘어간다. 기기 안 음성 인식이면 명령의 전사문이 보통 턴을 거친다.
    pub(crate) fn complete_answer(self: &Arc<Self>, command: &VoiceAnswerCommand) -> AppResult<()> {
        self.purge_contexts();
        let Some(voice_session_id) = command
            .voice_session_id
            .as_deref()
            .filter(|id| !java_is_blank(id))
        else {
            return Err(invalid("voiceSessionId is required"));
        };
        let context = self.require_context(voice_session_id)?;
        if context.settings.server_stt() {
            return Err(invalid(
                "This session transcribes uploaded audio; answer signals are not accepted",
            ));
        }
        if context.is_inactive() || !context.accept_chunk_sequence(command.sequence) {
            return Ok(());
        }
        if !context.settings.ai_enabled {
            return self.advance_after_answer(&context);
        }
        context.offer_transcript(command.text.as_deref());
        self.flush_transcript_turn(&context);
        Ok(())
    }

    pub(crate) fn stop(&self, command: &VoiceSessionStopCommand) -> AppResult<()> {
        self.purge_contexts();
        let Some(voice_session_id) = command
            .voice_session_id
            .as_deref()
            .filter(|id| !java_is_blank(id))
        else {
            return Err(invalid("voiceSessionId is required"));
        };
        let Some(context) = self.find_context(voice_session_id) else {
            return Ok(());
        };
        let reason = normalize_stop_reason(command.reason.as_deref(), "user_stop");
        self.stop_internal(&context, command.forced, &reason);
        Ok(())
    }

    /// 클라이언트 요청으로 찾은 세션은 방금 쓰인 것으로 적는다.
    fn find_context(&self, voice_session_id: &str) -> Option<Arc<RuntimeContext>> {
        let context = lock(&self.contexts).get(voice_session_id).cloned()?;
        context.touch();
        Some(context)
    }

    fn require_context(&self, voice_session_id: &str) -> AppResult<Arc<RuntimeContext>> {
        self.find_context(voice_session_id)
            .ok_or_else(|| invalid(&format!("Unknown voice session: {voice_session_id}")))
    }

    fn initialize_session(self: &Arc<Self>, context: &Arc<RuntimeContext>) {
        if context.is_inactive() {
            return;
        }
        let result = (|| {
            if context.settings.ai_enabled {
                self.deps.feedback_use_case.bootstrap_conversation(
                    &context.feedback_api_key,
                    &feedback_command(context, ""),
                    Some(&context.settings.feedback_language),
                )?;
            }
            context.emit(
                "session.ready",
                json!({
                    "sessionId": context.session_id,
                    "voiceSessionId": context.voice_session_id,
                    "aiEnabled": context.settings.ai_enabled,
                    "feedbackProvider": context.settings.feedback_provider,
                }),
            );
            self.send_question_prompt(context)
        })();
        if let Err(error) = result {
            context.emit(
                "session.error",
                error_payload(&context.session_id, 0, &error_message(&error)),
            );
            self.stop_internal(context, false, "initialization_failed");
        }
    }

    fn send_question_prompt(self: &Arc<Self>, context: &Arc<RuntimeContext>) -> AppResult<()> {
        let state = self
            .deps
            .session_use_case
            .get_or_create(&context.session_id)?;
        let next_question = self
            .deps
            .question_use_case
            .next_question(&context.session_id)?;
        let mode = lock_session(&state)?.mode();
        let exhausted = is_question_exhausted(&next_question);
        let turn_id = if exhausted { 0 } else { context.next_turn_id() };
        context.emit(
            "question.prompt",
            question_payload(&context.session_id, turn_id, mode, &next_question),
        );
        if exhausted {
            self.stop_internal(context, false, SessionStopReason::QuestionExhausted.code());
            return Ok(());
        }
        context.set_current_question(next_question.clone(), turn_id);
        self.schedule_turn_prefetch(context, &next_question);
        self.emit_speech(context, turn_id, "question", next_question.text.as_deref());
        Ok(())
    }

    /// 처음 닫을 때만 `session.stopped`를 보낸다.
    fn stop_internal(&self, context: &RuntimeContext, forced: bool, reason: &str) {
        if !context.close() {
            return;
        }
        let fallback = if forced {
            "user_stop"
        } else {
            SessionStopReason::QuestionExhausted.code()
        };
        let stop_reason = normalize_stop_reason(Some(reason), fallback);
        context.mark_stopped(&stop_reason, (self.now_millis)());
        context.release_pending_work();
        context.emit(
            "session.stopped",
            json!({
                "sessionId": context.session_id,
                "voiceSessionId": context.voice_session_id,
                "forced": forced,
                "reason": stop_reason,
            }),
        );
    }

    /// 버려진 진행 중 세션을 `idle_timeout`으로 멈추고, 보관 기간이 지난 정지 세션을 지우고,
    /// 정지 세션이 상한보다 많으면 오래된 것부터 지운다. 이벤트 스트림이 붙어 있거나 턴을
    /// 처리 중인 세션은 멈추지 않는다.
    pub(crate) fn purge_contexts(&self) {
        let now = (self.now_millis)();
        let mut idle_timeout_ms = self.deps.voice_policy.voice_idle_timeout_ms();
        if idle_timeout_ms <= 0 {
            idle_timeout_ms = DEFAULT_IDLE_TIMEOUT_MS;
        }
        let abandoned: Vec<Arc<RuntimeContext>> = lock(&self.contexts)
            .values()
            .filter(|context| context.is_abandoned(now, idle_timeout_ms))
            .cloned()
            .collect();
        // 정지는 이벤트를 보내므로 세션 목록 잠금을 놓은 뒤에 한다.
        for context in abandoned {
            tracing::info!(
                voice_session_id = %context.voice_session_id,
                "Stopping voice session without an event stream after {idle_timeout_ms} ms idle"
            );
            self.stop_internal(&context, true, IDLE_TIMEOUT_STOP_REASON);
        }
        self.purge_stopped_contexts(now);
    }

    /// 보관 기간이 지난 정지 세션을 지우고, 정지 세션이 상한보다 많으면 오래된 것부터 지운다.
    /// 진행 중인 세션은 지우지 않는다.
    fn purge_stopped_contexts(&self, now: i64) {
        let mut retention_ms = self.deps.voice_policy.voice_recovery_retention_ms();
        if retention_ms <= 0 {
            retention_ms = DEFAULT_RECOVERY_RETENTION_MS;
        }
        let mut stopped_context_max = self.deps.voice_policy.voice_stopped_context_max();
        if stopped_context_max == 0 {
            stopped_context_max = DEFAULT_STOPPED_CONTEXT_MAX;
        }

        let mut contexts = lock(&self.contexts);
        let mut retained: Vec<(String, i64)> = Vec::new();
        contexts.retain(|voice_session_id, context| {
            if !context.is_inactive() {
                return true;
            }
            if context.is_stopped_expired(now, retention_ms) {
                return false;
            }
            retained.push((voice_session_id.clone(), context.stopped_at_epoch_ms()));
            true
        });
        if retained.len() <= stopped_context_max {
            return;
        }
        retained.sort_by_key(|(_, stopped_at)| *stopped_at);
        let remove_count = retained.len() - stopped_context_max;
        for (voice_session_id, _) in retained.into_iter().take(remove_count) {
            contexts.remove(&voice_session_id);
        }
    }
}

fn feedback_command(context: &RuntimeContext, text: &str) -> FeedbackCommand {
    FeedbackCommand {
        session_id: Some(context.session_id.clone()),
        text: Some(text.to_string()),
        provider: Some(context.settings.feedback_provider.clone()),
        model: Some(context.settings.feedback_model.clone()),
        feedback_language: Some(context.settings.feedback_language.clone()),
    }
}

fn normalize_stop_reason(reason: Option<&str>, fallback: &str) -> String {
    match reason.filter(|reason| !java_is_blank(reason)) {
        Some(reason) => reason.to_string(),
        None => fallback.to_string(),
    }
}

fn is_blank(value: Option<&str>) -> bool {
    value.is_none_or(java_is_blank)
}

fn invalid(message: &str) -> AppError {
    AppError::InvalidArgument(message.to_string())
}
