//! SSE 이벤트 payload와 음성(TTS) 이벤트 발행.
//!
//! payload의 필드 이름, 중첩, 값의 타입은 Java(Jackson으로 직렬화한 `Map`)와 같다. Java가
//! `LinkedHashMap`에 null을 넣는 필드는 JSON null로, 넣지 않는 필드는 빼고 만든다. `eventId`는
//! `RuntimeContext::emit`이 붙인다.

use std::time::Instant;

use omypic_domain::error::{AppError, AppResult};
use omypic_domain::feedback::{CorrectionDetail, Feedback, RecommendationDetail};
use omypic_domain::question::QuestionGroup;
use omypic_domain::session::{
    BatchReason, ModeType, NextActionType, SessionAction, SessionStopReason,
};
use omypic_domain::shared::text_utils::{java_is_blank, java_trim};
use serde_json::{json, Value};

use super::audio::encode_base64;
use super::context::RuntimeContext;
use super::VoiceSessionFacade;
use crate::question::model::NextQuestion;

/// 피드백 하나와 그 질문/답.
pub(super) struct FeedbackItem {
    pub(super) question_id: Option<String>,
    pub(super) question_text: Option<String>,
    pub(super) question_group: Option<QuestionGroup>,
    pub(super) answer_text: String,
    pub(super) feedback: Feedback,
}

pub(super) fn mode_name(mode: ModeType) -> &'static str {
    match mode {
        ModeType::Immediate => "IMMEDIATE",
        ModeType::Continuous => "CONTINUOUS",
    }
}

fn batch_reason_name(reason: BatchReason) -> &'static str {
    match reason {
        BatchReason::ImmediateMode => "IMMEDIATE_MODE",
        BatchReason::WaitingForGroupCompletion => "WAITING_FOR_GROUP_COMPLETION",
        BatchReason::WaitingForBatch => "WAITING_FOR_BATCH",
        BatchReason::BatchReady => "BATCH_READY",
        BatchReason::ExhaustedWithRemainder => "EXHAUSTED_WITH_REMAINDER",
    }
}

pub(super) fn is_question_exhausted(question: &NextQuestion) -> bool {
    question.skipped
}

/// Java `defaultMessage`: 비었으면 `Unknown voice session error`.
pub(super) fn default_message(message: &str) -> String {
    if java_is_blank(message) {
        "Unknown voice session error".to_string()
    } else {
        message.to_string()
    }
}

pub(super) fn error_message(error: &AppError) -> String {
    default_message(&error.to_string())
}

/// `session.error`: `{sessionId, turnId, message}`.
pub(super) fn error_payload(session_id: &str, turn_id: i64, message: &str) -> Value {
    json!({ "sessionId": session_id, "turnId": turn_id, "message": message })
}

/// `question.prompt`: `{sessionId, turnId, question, selection}`. 질문이 다 떨어졌으면
/// `question`은 null이고 `selection.reason`이 붙는다.
pub(super) fn question_payload(
    session_id: &str,
    turn_id: i64,
    mode: ModeType,
    question: &NextQuestion,
) -> Value {
    let exhausted = is_question_exhausted(question);
    let node = if exhausted {
        Value::Null
    } else {
        json!({
            "id": question.question_id,
            "text": question.text,
            "group": question.group,
            "groupId": question.group_id,
            "questionType": question.question_type,
        })
    };
    let mut selection = json!({ "mode": mode_name(mode), "exhausted": exhausted });
    if exhausted {
        selection["reason"] = json!(SessionStopReason::QuestionExhausted.code());
    }
    json!({
        "sessionId": session_id,
        "turnId": turn_id,
        "question": node,
        "selection": selection,
    })
}

/// `feedback.final`: `{sessionId, turnId, policy, batch, nextAction}`.
#[allow(clippy::too_many_arguments)]
pub(super) fn feedback_payload(
    session_id: &str,
    turn_id: i64,
    mode: ModeType,
    group_batch_size: i32,
    reason: BatchReason,
    residual: bool,
    items: &[FeedbackItem],
    next_action: SessionAction,
) -> Value {
    json!({
        "sessionId": session_id,
        "turnId": turn_id,
        "policy": {
            "mode": mode_name(mode),
            "groupBatchSize": group_batch_size.max(1),
            "reason": batch_reason_name(reason),
        },
        "batch": {
            "size": items.len(),
            "isResidual": residual,
            "items": items.iter().map(feedback_item_payload).collect::<Vec<_>>(),
        },
        "nextAction": next_action_payload(next_action),
    })
}

fn feedback_item_payload(item: &FeedbackItem) -> Value {
    let feedback = &item.feedback;
    let corrections = feedback.corrections();
    let recommendations = feedback.recommendations();
    json!({
        "questionId": item.question_id,
        "questionText": item.question_text,
        "questionGroup": item.question_group.as_ref().map(QuestionGroup::value),
        "answerText": item.answer_text,
        "summary": feedback.summary(),
        "corrections": {
            "grammar": correction_detail_payload(corrections.grammar()),
            "expression": correction_detail_payload(corrections.expression()),
            "logic": correction_detail_payload(corrections.logic()),
        },
        "recommendations": {
            "filler": recommendation_detail_payload(recommendations.filler()),
            "adjective": recommendation_detail_payload(recommendations.adjective()),
            "adverb": recommendation_detail_payload(recommendations.adverb()),
        },
        "exampleAnswer": feedback.example_answer(),
        "rulebookEvidence": feedback.rulebook_evidence(),
    })
}

fn correction_detail_payload(detail: &CorrectionDetail) -> Value {
    json!({ "issue": detail.issue(), "fix": detail.fix() })
}

fn recommendation_detail_payload(detail: &RecommendationDetail) -> Value {
    json!({ "term": detail.term(), "usage": detail.usage() })
}

/// `{type: "ask_next" | "auto_stop", reason?}`. 이유가 없으면 `reason`을 넣지 않는다.
fn next_action_payload(action: SessionAction) -> Value {
    let action_type = match action.action_type() {
        NextActionType::AutoStop => "auto_stop",
        NextActionType::AskNext => "ask_next",
    };
    let mut payload = json!({ "type": action_type });
    if let Some(reason) = action.reason() {
        payload["reason"] = json!(reason.code());
    }
    payload
}

/// 여러 피드백을 읽을 글로 만든다.
pub(super) fn feedback_items_tts_text(items: &[FeedbackItem]) -> String {
    match items {
        [] => "No feedback available.".to_string(),
        [item] => feedback_tts_text(&item.feedback),
        _ => {
            let mut builder = String::new();
            for (index, item) in items.iter().enumerate() {
                builder.push_str(&format!("Feedback {}\n", index + 1));
                if let Some(text) = item.question_text.as_deref().filter(|t| !java_is_blank(t)) {
                    builder.push_str(text);
                    builder.push('\n');
                }
                builder.push_str(&feedback_tts_text(&item.feedback));
                builder.push('\n');
            }
            java_trim(&builder).to_string()
        }
    }
}

fn feedback_tts_text(feedback: &Feedback) -> String {
    let sections: Vec<String> = [
        feedback.summary().to_string(),
        feedback.correction_points().join("\n"),
        feedback.recommendation().join("\n"),
        feedback.example_answer().to_string(),
    ]
    .into_iter()
    .filter(|section| !java_is_blank(section))
    .collect();
    let text = java_trim(&sections.join("\n")).to_string();
    if text.is_empty() {
        "No feedback available.".to_string()
    } else {
        text
    }
}

impl VoiceSessionFacade {
    /// 읽을 글을 보낸다. 서버 TTS면 `tts.audio`(실패하면 `tts.error`), 아니면 클라이언트가 읽을
    /// `tts.text`다. 세션이 닫혔거나 글이 비었으면 보내지 않는다.
    pub(super) fn emit_speech(
        &self,
        context: &RuntimeContext,
        turn_id: i64,
        role: &str,
        text: Option<&str>,
    ) {
        let Some(text) = text.filter(|text| !java_is_blank(text)) else {
            return;
        };
        if context.is_inactive() {
            return;
        }
        let phase = if java_is_blank(role) {
            "question"
        } else {
            role
        };
        if !context.settings.server_tts() {
            context.emit(
                "tts.text",
                json!({
                    "sessionId": context.session_id,
                    "turnId": turn_id,
                    "role": phase,
                    "phase": phase,
                    "sequence": context.next_tts_sequence(),
                    "text": text,
                }),
            );
            return;
        }
        if let Err(error) = self.emit_speech_audio(context, turn_id, phase, text) {
            self.deps.metrics.increment_tts_error();
            context.emit(
                "tts.error",
                json!({
                    "sessionId": context.session_id,
                    "turnId": turn_id,
                    "message": error_message(&error),
                }),
            );
        }
    }

    fn emit_speech_audio(
        &self,
        context: &RuntimeContext,
        turn_id: i64,
        phase: &str,
        text: &str,
    ) -> AppResult<()> {
        let start = Instant::now();
        let wav = self.deps.tts_gateway.synthesize(
            &context.api_key,
            &context.settings.tts_model,
            &context.settings.tts_voice,
            text,
        )?;
        if wav.is_empty() {
            return Ok(());
        }
        self.deps.metrics.record_tts_latency(start.elapsed());
        context.emit(
            "tts.audio",
            json!({
                "sessionId": context.session_id,
                "turnId": turn_id,
                "role": phase,
                "phase": phase,
                "sequence": context.next_tts_sequence(),
                "text": text,
                "audio": encode_base64(&wav),
                "mimeType": "audio/wav",
            }),
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omypic_domain::session::SessionFlowPolicy;

    #[test]
    fn question_payload_has_null_question_and_reason_when_exhausted() {
        let payload = question_payload("s1", 3, ModeType::Continuous, &NextQuestion::skipped());

        assert_eq!(
            json!({
                "sessionId": "s1",
                "turnId": 3,
                "question": null,
                "selection": {"mode": "CONTINUOUS", "exhausted": true, "reason": "QUESTION_EXHAUSTED"},
            }),
            payload
        );
    }

    #[test]
    fn question_payload_keeps_null_question_fields() {
        let question = NextQuestion {
            question_id: Some("q-1".to_string()),
            text: Some("text".to_string()),
            ..NextQuestion::default()
        };

        let payload = question_payload("s1", 1, ModeType::Immediate, &question);

        assert_eq!(
            json!({
                "sessionId": "s1",
                "turnId": 1,
                "question": {"id": "q-1", "text": "text", "group": null, "groupId": null, "questionType": null},
                "selection": {"mode": "IMMEDIATE", "exhausted": false},
            }),
            payload
        );
    }

    #[test]
    fn feedback_payload_matches_java_shape() {
        let feedback = Feedback::of_correction_points(
            "summary",
            &["point-1".to_string()],
            "example",
            &["evidence".to_string()],
        );
        let item = FeedbackItem {
            question_id: None,
            question_text: Some("question".to_string()),
            question_group: None,
            answer_text: "answer".to_string(),
            feedback: feedback.clone(),
        };

        let payload = feedback_payload(
            "s1",
            2,
            ModeType::Continuous,
            0,
            BatchReason::ExhaustedWithRemainder,
            true,
            &[item],
            SessionFlowPolicy::decide_after_question_selection(true),
        );

        let corrections = feedback.corrections();
        let recommendations = feedback.recommendations();
        assert_eq!(
            json!({
                "sessionId": "s1",
                "turnId": 2,
                "policy": {"mode": "CONTINUOUS", "groupBatchSize": 1, "reason": "EXHAUSTED_WITH_REMAINDER"},
                "batch": {
                    "size": 1,
                    "isResidual": true,
                    "items": [{
                        "questionId": null,
                        "questionText": "question",
                        "questionGroup": null,
                        "answerText": "answer",
                        "summary": "summary",
                        "corrections": {
                            "grammar": {"issue": corrections.grammar().issue(), "fix": corrections.grammar().fix()},
                            "expression": {"issue": corrections.expression().issue(), "fix": corrections.expression().fix()},
                            "logic": {"issue": corrections.logic().issue(), "fix": corrections.logic().fix()},
                        },
                        "recommendations": {
                            "filler": {"term": recommendations.filler().term(), "usage": recommendations.filler().usage()},
                            "adjective": {"term": recommendations.adjective().term(), "usage": recommendations.adjective().usage()},
                            "adverb": {"term": recommendations.adverb().term(), "usage": recommendations.adverb().usage()},
                        },
                        "exampleAnswer": "example",
                        "rulebookEvidence": ["evidence"],
                    }],
                },
                "nextAction": {"type": "auto_stop", "reason": "QUESTION_EXHAUSTED"},
            }),
            payload
        );
    }

    #[test]
    fn next_action_payload_omits_reason_for_ask_next() {
        assert_eq!(
            json!({"type": "ask_next"}),
            next_action_payload(SessionAction::ask_next())
        );
    }

    #[test]
    fn tts_text_joins_numbered_feedback_for_several_items() {
        let item = |question: Option<&str>| FeedbackItem {
            question_id: None,
            question_text: question.map(str::to_string),
            question_group: None,
            answer_text: String::new(),
            feedback: Feedback::of_correction_points("summary", &[], "", &[]),
        };

        assert_eq!("No feedback available.", feedback_items_tts_text(&[]));
        let text = feedback_items_tts_text(&[item(Some("Q1")), item(None)]);
        assert!(text.starts_with("Feedback 1\nQ1\nsummary"), "{text}");
        assert!(text.contains("\nFeedback 2\nsummary"), "{text}");
        assert!(!text.ends_with('\n'));
    }
}
