use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::sse::Sse;
use axum::routing::{get, post};
use axum::{Json, Router};
use omypic_application::voice::model::{
    VoiceAnswerCommand, VoiceAudioChunkCommand, VoiceSessionOpenCommand, VoiceSessionStopCommand,
};
use omypic_application::voice::usecase::{RecoveryQuestion, RecoverySnapshot};
use serde::{Deserialize, Serialize};

use crate::error::{blocking, require_not_blank, ApiError, ApiJson, OptionalJson};
use crate::sse::{open_event_stream, EventStream, SSE_TIMEOUT};
use crate::state::AppState;

const API_KEY_HEADER: &str = "X-API-Key";
const FEEDBACK_API_KEY_HEADER: &str = "X-Feedback-API-Key";

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/voice/sessions", post(open))
        .route("/api/voice/sessions/{voice_session_id}/events", get(events))
        .route(
            "/api/voice/sessions/{voice_session_id}/recovery",
            get(recover),
        )
        .route(
            "/api/voice/sessions/{voice_session_id}/audio-chunks",
            post(append_audio),
        )
        .route(
            "/api/voice/sessions/{voice_session_id}/answers",
            post(complete_answer),
        )
        .route("/api/voice/sessions/{voice_session_id}/stop", post(stop))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct VoiceSessionCreateRequest {
    session_id: Option<String>,
    feedback_model: Option<String>,
    feedback_provider: Option<String>,
    ai_enabled: Option<bool>,
    feedback_language: Option<String>,
    stt_model: Option<String>,
    tts_model: Option<String>,
    tts_voice: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VoiceSessionCreateResponse {
    voice_session_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct VoiceAudioChunkRequest {
    pcm16_base64: Option<String>,
    sample_rate: Option<i32>,
    sequence: Option<i64>,
}

#[derive(Deserialize)]
struct VoiceAnswerRequest {
    sequence: Option<i64>,
    text: Option<String>,
}

#[derive(Deserialize)]
struct VoiceSessionStopRequest {
    forced: Option<bool>,
    reason: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VoiceSessionRecoveryResponse {
    session_id: String,
    voice_session_id: String,
    active: bool,
    stopped: bool,
    stop_reason: String,
    current_turn_id: i64,
    current_question: Option<RecoveryQuestionResponse>,
    turn_processing: bool,
    has_buffered_audio: bool,
    last_accepted_chunk_sequence: Option<i64>,
    latest_event_id: i64,
    replay_from_event_id: i64,
    gap_detected: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RecoveryQuestionResponse {
    id: Option<String>,
    text: Option<String>,
    group: Option<String>,
    group_id: Option<String>,
    question_type: Option<String>,
}

impl From<RecoveryQuestion> for RecoveryQuestionResponse {
    fn from(question: RecoveryQuestion) -> Self {
        Self {
            id: question.id,
            text: question.text,
            group: question.group,
            group_id: question.group_id,
            question_type: question.question_type,
        }
    }
}

impl From<RecoverySnapshot> for VoiceSessionRecoveryResponse {
    fn from(snapshot: RecoverySnapshot) -> Self {
        Self {
            session_id: snapshot.session_id,
            voice_session_id: snapshot.voice_session_id,
            active: snapshot.active,
            stopped: snapshot.stopped,
            stop_reason: snapshot.stop_reason,
            current_turn_id: snapshot.current_turn_id,
            current_question: snapshot.current_question.map(Into::into),
            turn_processing: snapshot.turn_processing,
            has_buffered_audio: snapshot.has_buffered_audio,
            last_accepted_chunk_sequence: snapshot.last_accepted_chunk_sequence,
            latest_event_id: snapshot.latest_event_id,
            replay_from_event_id: snapshot.replay_from_event_id,
            gap_detected: snapshot.gap_detected,
        }
    }
}

/// 헤더 값은 로그에 남기지 않는다. UTF-8이 아니면 없는 것으로 본다.
fn header_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
}

/// `Long` 질의 파라미터. 없거나 비어 있으면 `None`, 숫자가 아니면 400이다.
fn long_param(params: &HashMap<String, String>, name: &str) -> Result<Option<i64>, ApiError> {
    match params.get(name).map(|value| value.trim()) {
        None | Some("") => Ok(None),
        Some(text) => text
            .parse()
            .map(Some)
            .map_err(|_| ApiError::BadRequest(format!("Invalid value for '{name}'"))),
    }
}

async fn open(
    State(state): State<AppState>,
    headers: HeaderMap,
    ApiJson(request): ApiJson<VoiceSessionCreateRequest>,
) -> Result<Json<VoiceSessionCreateResponse>, ApiError> {
    require_not_blank("sessionId", &request.session_id)?;
    let command = VoiceSessionOpenCommand {
        session_id: request.session_id,
        api_key: header_value(&headers, API_KEY_HEADER),
        feedback_api_key: header_value(&headers, FEEDBACK_API_KEY_HEADER),
        feedback_provider: request.feedback_provider,
        ai_enabled: request.ai_enabled,
        feedback_model: request.feedback_model,
        feedback_language: request.feedback_language,
        stt_model: request.stt_model,
        tts_model: request.tts_model,
        tts_voice: request.tts_voice,
    };
    let use_case = state.voice_session.clone();
    let voice_session_id = blocking(move || use_case.open(&command)).await?;
    Ok(Json(VoiceSessionCreateResponse { voice_session_id }))
}

async fn events(
    State(state): State<AppState>,
    Path(voice_session_id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Sse<EventStream>, ApiError> {
    let since_event_id = long_param(&params, "sinceEventId")?;
    open_event_stream(
        Arc::clone(&state.voice_session) as _,
        voice_session_id,
        since_event_id,
        SSE_TIMEOUT,
    )
    .await
}

async fn recover(
    State(state): State<AppState>,
    Path(voice_session_id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<VoiceSessionRecoveryResponse>, ApiError> {
    let last_seen_event_id = long_param(&params, "lastSeenEventId")?;
    let use_case = state.voice_session.clone();
    let snapshot =
        blocking(move || use_case.recover(&voice_session_id, last_seen_event_id)).await?;
    Ok(Json(snapshot.into()))
}

async fn append_audio(
    State(state): State<AppState>,
    Path(voice_session_id): Path<String>,
    ApiJson(request): ApiJson<VoiceAudioChunkRequest>,
) -> Result<StatusCode, ApiError> {
    require_not_blank("pcm16Base64", &request.pcm16_base64)?;
    let command = VoiceAudioChunkCommand {
        voice_session_id: Some(voice_session_id),
        pcm16_base64: request.pcm16_base64,
        sample_rate: request.sample_rate,
        sequence: request.sequence,
    };
    let use_case = state.voice_session.clone();
    blocking(move || use_case.append_audio(&command)).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn complete_answer(
    State(state): State<AppState>,
    Path(voice_session_id): Path<String>,
    OptionalJson(request): OptionalJson<VoiceAnswerRequest>,
) -> Result<StatusCode, ApiError> {
    let command = VoiceAnswerCommand {
        voice_session_id: Some(voice_session_id),
        sequence: request.as_ref().and_then(|r| r.sequence),
        text: request.and_then(|r| r.text),
    };
    let use_case = state.voice_session.clone();
    blocking(move || use_case.complete_answer(&command)).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn stop(
    State(state): State<AppState>,
    Path(voice_session_id): Path<String>,
    OptionalJson(request): OptionalJson<VoiceSessionStopRequest>,
) -> Result<StatusCode, ApiError> {
    let command = match request {
        None => VoiceSessionStopCommand {
            voice_session_id: Some(voice_session_id),
            forced: true,
            reason: Some("user_stop".to_string()),
        },
        Some(body) => VoiceSessionStopCommand {
            voice_session_id: Some(voice_session_id),
            forced: body.forced.unwrap_or(true),
            reason: body.reason,
        },
    };
    let use_case = state.voice_session.clone();
    blocking(move || use_case.stop(&command)).await?;
    Ok(StatusCode::NO_CONTENT)
}
