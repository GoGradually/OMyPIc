//! 턴 처리: 답(오디오 또는 전사문)을 받아 STT, 피드백, 다음 질문, 자동 정지까지.
//!
//! 한 세션에서 턴은 한 번에 하나만 처리한다(`start_turn_processing`). 처리 중에 온 오디오와
//! 전사문은 쌓아 두었다가 턴이 끝나면 이어서 처리한다.
//!
//! 턴 안에서 패닉이 나도 처리 중 표시를 풀고 `session.error`를 보낸다(`catch_turn_panic`).
//! 세션이 멈춘 뒤에 끝난 턴은 STT 다음부터 아무 일도 하지 않는다(LLM 호출, 오답노트, 다음 질문,
//! 이벤트 모두 없음).

use std::any::Any;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;
use std::time::Instant;

use omypic_domain::error::{AppError, AppResult};
use omypic_domain::feedback::{Feedback, FeedbackLanguage};
use omypic_domain::session::{
    BatchReason, ModeType, NextActionType, SessionAction, SessionFlowPolicy,
};
use omypic_domain::shared::text_utils::java_is_blank;
use serde_json::json;

use super::audio::to_wav;
use super::context::{AudioSnapshot, QuestionSnapshot, RuntimeContext, TurnInput};
use super::events::{
    error_message, error_payload, feedback_items_tts_text, feedback_payload, is_question_exhausted,
    question_payload, FeedbackItem,
};
use super::{feedback_command, VoiceSessionFacade, MAX_RULEBOOK_DOCUMENTS_PER_TURN};
use crate::question::model::NextQuestion;
use crate::session::port::{lock_session, SharedSessionState};
use crate::stt::model::SttCommand;

struct FeedbackPlan {
    inputs: Vec<TurnInput>,
    reason: BatchReason,
}

impl VoiceSessionFacade {
    /// 쌓인 오디오로 턴 하나를 시작한다. 이미 처리 중이면 오디오는 버퍼에 남는다.
    pub(super) fn flush_turn(self: &Arc<Self>, context: &Arc<RuntimeContext>) {
        if context.is_inactive() || !context.start_turn_processing() {
            return;
        }
        let snapshot = context.consume_audio_snapshot();
        if snapshot.pcm16.is_empty() || snapshot.question.is_none() {
            self.complete_turn_processing(context);
            return;
        }
        let this = Arc::clone(self);
        let task_context = Arc::clone(context);
        self.deps
            .async_executor
            .execute(Box::new(move || this.process_turn(&task_context, snapshot)));
    }

    pub(super) fn flush_transcript_turn(self: &Arc<Self>, context: &Arc<RuntimeContext>) {
        if context.is_inactive() || !context.start_turn_processing() {
            return;
        }
        let turn = match context.poll_transcript() {
            Some(turn) if turn.question.is_some() => turn,
            _ => {
                self.complete_turn_processing(context);
                return;
            }
        };
        let this = Arc::clone(self);
        let task_context = Arc::clone(context);
        self.deps.async_executor.execute(Box::new(move || {
            let started_at = Instant::now();
            let result = catch_turn_panic(&task_context, turn.turn_id, || {
                let question = turn.question.as_ref().expect("checked above");
                this.process_transcript(
                    &task_context,
                    question,
                    &turn.text,
                    turn.turn_id,
                    started_at,
                )
            });
            if let Err(error) = result {
                task_context.emit(
                    "session.error",
                    error_payload(
                        &task_context.session_id,
                        turn.turn_id,
                        &error_message(&error),
                    ),
                );
            }
            this.complete_turn_processing(&task_context);
        }));
    }

    /// AI가 꺼진 세션: 전사도 피드백도 없이 다음 질문으로 넘어간다. 호출한 스레드에서 돈다.
    pub(super) fn advance_after_answer(
        self: &Arc<Self>,
        context: &Arc<RuntimeContext>,
    ) -> AppResult<()> {
        if !context.start_turn_processing() {
            return Ok(());
        }
        let mut turn_id = 0;
        let result = catch_unwind(AssertUnwindSafe(|| {
            let snapshot = context.consume_audio_snapshot();
            turn_id = snapshot.turn_id;
            if snapshot.question.is_none() {
                return Ok(());
            }
            context.emit(
                "answer.captured",
                json!({ "sessionId": context.session_id, "turnId": snapshot.turn_id }),
            );
            let state = self
                .deps
                .session_use_case
                .get_or_create(&context.session_id)?;
            let mode = lock_session(&state).mode();
            self.advance_without_feedback(context, mode, snapshot.turn_id)
        }));
        context.finish_turn_processing();
        result.unwrap_or_else(|payload| {
            let error = turn_panic_error(context, turn_id, payload.as_ref());
            context.emit(
                "session.error",
                error_payload(&context.session_id, turn_id, &error_message(&error)),
            );
            Err(error)
        })
    }

    fn process_turn(self: &Arc<Self>, context: &Arc<RuntimeContext>, snapshot: AudioSnapshot) {
        let started_at = Instant::now();
        let turn_id = snapshot.turn_id;
        let result = catch_turn_panic(context, turn_id, || {
            let transcript = self.transcribe(&snapshot, context)?;
            let question = snapshot.question.as_ref().expect("checked by flush_turn");
            self.process_transcript(context, question, &transcript, turn_id, started_at)
        });
        if let Err(error) = result {
            context.emit(
                "session.error",
                error_payload(&context.session_id, turn_id, &error_message(&error)),
            );
        }
        self.complete_turn_processing(context);
    }

    fn complete_turn_processing(self: &Arc<Self>, context: &Arc<RuntimeContext>) {
        context.finish_turn_processing();
        if context.has_buffered_audio() {
            self.flush_turn(context);
        } else if context.has_pending_transcript() {
            self.flush_transcript_turn(context);
        }
    }

    fn transcribe(&self, snapshot: &AudioSnapshot, context: &RuntimeContext) -> AppResult<String> {
        let command = SttCommand {
            file_bytes: Some(to_wav(&snapshot.pcm16, snapshot.sample_rate)),
            model: context.settings.stt_model.clone(),
            api_key: context.api_key.clone(),
            translate: false,
            session_id: Some(context.session_id.clone()),
        };
        self.deps.stt_use_case.transcribe(&command)
    }

    fn process_transcript(
        self: &Arc<Self>,
        context: &Arc<RuntimeContext>,
        question: &QuestionSnapshot,
        transcript: &str,
        turn_id: i64,
        started_at: Instant,
    ) -> AppResult<()> {
        // STT가 끝나기 전에 세션이 멈췄으면 결과를 버린다.
        if context.is_inactive() {
            return Ok(());
        }
        if java_is_blank(transcript) {
            context.emit(
                "stt.skipped",
                json!({
                    "sessionId": context.session_id,
                    "turnId": turn_id,
                    "reason": "empty_transcript",
                }),
            );
            context.emit(
                "session.error",
                error_payload(&context.session_id, turn_id, "음성이 인식되지 않았습니다."),
            );
            return Ok(());
        }
        context.emit(
            "stt.final",
            json!({ "sessionId": context.session_id, "turnId": turn_id, "text": transcript }),
        );
        self.process_turn_flow(context, question, transcript, turn_id)?;
        self.deps
            .metrics
            .record_voice_turn_latency(started_at.elapsed());
        Ok(())
    }

    /// 즉시 모드는 피드백을 먼저 보내고 다음 질문을 보낸다. 연속 모드는 다음 질문을 먼저 보내고
    /// 묶음이 찼을 때만 피드백을 글로만 보낸다. 피드백이 실패해도 세션은 이어진다.
    fn process_turn_flow(
        self: &Arc<Self>,
        context: &Arc<RuntimeContext>,
        current_question: &QuestionSnapshot,
        answer_text: &str,
        turn_id: i64,
    ) -> AppResult<()> {
        let state = self
            .deps
            .session_use_case
            .get_or_create(&context.session_id)?;
        let mode = {
            let mut session = lock_session(&state);
            let mode = session.mode();
            session
                .set_feedback_language(FeedbackLanguage::of(&context.settings.feedback_language));
            mode
        };
        let turn_input = TurnInput {
            question_id: current_question.question_id.clone(),
            question_text: current_question.text.clone(),
            group_id: current_question.group_id.clone(),
            question_group_text: current_question.group.clone(),
            answer_text: answer_text.to_string(),
        };

        // 다음 질문을 꺼내면 세션의 질문 순서가 앞으로 간다. 멈춘 세션에서는 꺼내지 않는다.
        if context.is_inactive() {
            return Ok(());
        }
        let next_question = self
            .deps
            .question_use_case
            .next_question(&context.session_id)?;
        let exhausted = is_question_exhausted(&next_question);
        let next_action = SessionFlowPolicy::decide_after_question_selection(exhausted);

        let plan = self.resolve_feedback_plan(
            context,
            turn_input,
            &state,
            mode,
            &next_question,
            exhausted,
        )?;
        let emit_feedback = || {
            if context.is_inactive() {
                return;
            }
            let result = self
                .emit_main_feedback(context, turn_id, mode, &plan, next_action)
                .and_then(|emitted| {
                    self.emit_residual_feedback(
                        context,
                        turn_id,
                        mode,
                        &state,
                        next_action,
                        exhausted,
                        emitted,
                    )
                });
            // 세션이 멈춰서 그만둔 것이면 알리지 않는다.
            if let Err(error) = result {
                if !context.is_inactive() {
                    emit_feedback_error_and_continue(context, turn_id, &error);
                }
            }
        };
        let emit_next_question = || {
            if context.is_inactive() {
                return;
            }
            let next_turn_id = self.emit_question_prompt_and_track(
                context,
                mode,
                &next_question,
                exhausted,
                turn_id,
            );
            if next_turn_id > 0 {
                self.emit_speech(
                    context,
                    next_turn_id,
                    "question",
                    next_question.text.as_deref(),
                );
            }
        };
        if mode == ModeType::Immediate {
            emit_feedback();
            emit_next_question();
        } else {
            emit_next_question();
            emit_feedback();
        }

        self.stop_if_auto_stop(context, next_action);
        Ok(())
    }

    /// AI가 꺼진 세션: STT, 합성 API, LLM을 부르지 않는다. 다음 질문은 글로 가고 클라이언트가
    /// 로컬 Kokoro 음성으로 읽는다.
    fn advance_without_feedback(
        self: &Arc<Self>,
        context: &Arc<RuntimeContext>,
        mode: ModeType,
        turn_id: i64,
    ) -> AppResult<()> {
        let next_question = self
            .deps
            .question_use_case
            .next_question(&context.session_id)?;
        let exhausted = is_question_exhausted(&next_question);
        let next_turn_id =
            self.emit_question_prompt_and_track(context, mode, &next_question, exhausted, turn_id);
        if next_turn_id > 0 {
            self.emit_speech(
                context,
                next_turn_id,
                "question",
                next_question.text.as_deref(),
            );
        }
        self.stop_if_auto_stop(
            context,
            SessionFlowPolicy::decide_after_question_selection(exhausted),
        );
        Ok(())
    }

    fn stop_if_auto_stop(&self, context: &RuntimeContext, next_action: SessionAction) {
        if next_action.action_type() == NextActionType::AutoStop {
            let reason = next_action.reason().map_or("", |reason| reason.code());
            self.stop_internal(context, false, reason);
        }
    }

    /// 다음 질문을 보내고 현재 질문으로 둔다. 다 떨어졌으면 이전 턴 id로 보내고 0을 돌려준다.
    fn emit_question_prompt_and_track(
        self: &Arc<Self>,
        context: &Arc<RuntimeContext>,
        mode: ModeType,
        next_question: &NextQuestion,
        exhausted: bool,
        fallback_turn_id: i64,
    ) -> i64 {
        if exhausted {
            context.emit(
                "question.prompt",
                question_payload(&context.session_id, fallback_turn_id, mode, next_question),
            );
            context.clear_current_question();
            return 0;
        }
        let next_turn_id = context.next_turn_id();
        context.set_current_question(next_question.clone(), next_turn_id);
        self.schedule_turn_prefetch(context, next_question);
        context.emit(
            "question.prompt",
            question_payload(&context.session_id, next_turn_id, mode, next_question),
        );
        next_turn_id
    }

    fn resolve_feedback_plan(
        &self,
        context: &RuntimeContext,
        turn_input: TurnInput,
        state: &SharedSessionState,
        mode: ModeType,
        next_question: &NextQuestion,
        exhausted: bool,
    ) -> AppResult<FeedbackPlan> {
        if mode == ModeType::Immediate {
            return Ok(FeedbackPlan {
                inputs: vec![turn_input],
                reason: BatchReason::ImmediateMode,
            });
        }
        let completed_group = is_continuous_group_completed(&turn_input, next_question, exhausted);
        context.append_continuous_turn(turn_input);
        let decision = lock_session(state).decide_feedback_batch_on_turn(completed_group);
        let inputs = if decision.emit_feedback() {
            context.poll_all_continuous_turns()
        } else {
            Vec::new()
        };
        Ok(FeedbackPlan {
            inputs,
            reason: decision.reason(),
        })
    }

    fn emit_main_feedback(
        &self,
        context: &RuntimeContext,
        turn_id: i64,
        mode: ModeType,
        plan: &FeedbackPlan,
        next_action: SessionAction,
    ) -> AppResult<bool> {
        if plan.inputs.is_empty() {
            return Ok(false);
        }
        let items = self.generate_feedback_items(context, &plan.inputs)?;
        if context.is_inactive() {
            return Ok(true);
        }
        let state = self
            .deps
            .session_use_case
            .get_or_create(&context.session_id)?;
        let group_batch_size = group_batch_size(mode, &state)?;
        context.emit(
            "feedback.final",
            feedback_payload(
                &context.session_id,
                turn_id,
                mode,
                group_batch_size,
                plan.reason,
                false,
                &items,
                next_action,
            ),
        );
        if mode != ModeType::Continuous {
            self.emit_speech(
                context,
                turn_id,
                "feedback",
                Some(&feedback_items_tts_text(&items)),
            );
        }
        Ok(true)
    }

    /// 연속 모드에서 질문이 다 떨어졌는데 이번 턴에 피드백을 보내지 않았으면 남은 턴의 피드백을
    /// 보낸다.
    #[allow(clippy::too_many_arguments)]
    fn emit_residual_feedback(
        &self,
        context: &RuntimeContext,
        turn_id: i64,
        mode: ModeType,
        state: &SharedSessionState,
        next_action: SessionAction,
        exhausted: bool,
        emitted_feedback: bool,
    ) -> AppResult<()> {
        if !lock_session(state)
            .should_generate_residual_continuous_feedback(exhausted, emitted_feedback)
        {
            return Ok(());
        }
        let remainder = context.poll_all_continuous_turns();
        if remainder.is_empty() {
            return Ok(());
        }
        let items = self.generate_feedback_items(context, &remainder)?;
        if context.is_inactive() {
            return Ok(());
        }
        context.emit(
            "feedback.final",
            feedback_payload(
                &context.session_id,
                turn_id,
                mode,
                group_batch_size(mode, state)?,
                BatchReason::ExhaustedWithRemainder,
                true,
                &items,
                next_action,
            ),
        );
        Ok(())
    }

    fn generate_feedback_items(
        &self,
        context: &RuntimeContext,
        inputs: &[TurnInput],
    ) -> AppResult<Vec<FeedbackItem>> {
        inputs
            .iter()
            .map(|input| {
                // 묶음의 피드백을 만드는 동안 세션이 멈추면 남은 LLM 호출을 하지 않는다.
                if context.is_inactive() {
                    return Err(AppError::InvalidState(SESSION_STOPPED.to_string()));
                }
                Ok(FeedbackItem {
                    question_id: input.question_id.clone(),
                    question_text: input.question_text.clone(),
                    question_group: input.question_group(),
                    answer_text: input.answer_text.clone(),
                    feedback: self.generate_feedback(context, input)?,
                })
            })
            .collect()
    }

    /// 이 질문의 프리페치가 있으면 그것으로, 없으면 룰북을 직접 찾아 피드백을 만든다.
    fn generate_feedback(
        &self,
        context: &RuntimeContext,
        input: &TurnInput,
    ) -> AppResult<Feedback> {
        let command = feedback_command(context, &input.answer_text);
        let feedback_use_case = &self.deps.feedback_use_case;
        match context.consume_prefetched_turn_prompt(input.question_id.as_deref()) {
            Some(prefetch) => feedback_use_case.generate_feedback_for_turn_with_prefetch(
                &context.feedback_api_key,
                &command,
                &input.answer_text,
                Some(&prefetch),
            ),
            None => feedback_use_case.generate_feedback_for_turn(
                &context.feedback_api_key,
                &command,
                input.question_text.as_deref().unwrap_or(""),
                input.question_group().as_ref(),
                &input.answer_text,
                MAX_RULEBOOK_DOCUMENTS_PER_TURN,
            ),
        }
    }
}

const SESSION_STOPPED: &str = "Voice session stopped";
const TURN_PANIC_MESSAGE: &str = "음성 처리 중 예기치 않은 오류가 발생했습니다.";

/// `body`가 패닉하면 잡아서 로그를 남기고 오류로 돌려준다. 부르는 쪽은 다른 오류처럼
/// `session.error`를 보내고 처리 중 표시를 푼다.
fn catch_turn_panic(
    context: &RuntimeContext,
    turn_id: i64,
    body: impl FnOnce() -> AppResult<()>,
) -> AppResult<()> {
    catch_unwind(AssertUnwindSafe(body))
        .unwrap_or_else(|payload| Err(turn_panic_error(context, turn_id, payload.as_ref())))
}

fn turn_panic_error(
    context: &RuntimeContext,
    turn_id: i64,
    payload: &(dyn Any + Send),
) -> AppError {
    let detail = payload
        .downcast_ref::<&str>()
        .map(|message| (*message).to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic payload".to_string());
    tracing::error!(
        "voice turn panicked sessionId={} turnId={} reason={}",
        context.session_id,
        turn_id,
        detail
    );
    AppError::Internal(TURN_PANIC_MESSAGE.to_string())
}

fn group_batch_size(mode: ModeType, state: &SharedSessionState) -> AppResult<i32> {
    if mode == ModeType::Continuous {
        return Ok(lock_session(state).continuous_batch_size());
    }
    Ok(1)
}

/// 이번 답으로 질문 그룹 하나가 끝났는가. 다음 질문이 없거나 다른 그룹이면 끝난 것이다.
fn is_continuous_group_completed(
    turn_input: &TurnInput,
    next_question: &NextQuestion,
    exhausted: bool,
) -> bool {
    let Some(group_id) = turn_input
        .group_id
        .as_deref()
        .filter(|id| !java_is_blank(id))
    else {
        return false;
    };
    if exhausted || next_question.skipped {
        return true;
    }
    match next_question
        .group_id
        .as_deref()
        .filter(|id| !java_is_blank(id))
    {
        Some(next_group_id) => group_id != next_group_id,
        None => false,
    }
}

fn emit_feedback_error_and_continue(context: &RuntimeContext, turn_id: i64, error: &AppError) {
    let failure = error_message(error);
    context.emit(
        "session.error",
        error_payload(
            &context.session_id,
            turn_id,
            &format!("피드백 생성에 실패했습니다. 다음 질문으로 진행합니다. ({failure})"),
        ),
    );
    tracing::warn!(
        "feedback generation failed but session continued sessionId={} turnId={} reason={}",
        context.session_id,
        turn_id,
        failure
    );
}
