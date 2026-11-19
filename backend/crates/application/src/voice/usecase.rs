//! Java `VoiceSessionUseCase`: 음성 세션의 공개 진입점. 동작은 `facade`에 있다.

#[cfg(test)]
mod tests;
#[cfg(test)]
mod validation_tests;

use std::sync::Arc;

use chrono::Utc;
use omypic_domain::error::AppResult;

use super::facade::VoiceSessionFacade;
use super::model::{
    VoiceAnswerCommand, VoiceAudioChunkCommand, VoiceEventSink, VoiceSessionOpenCommand,
    VoiceSessionStopCommand,
};
use super::policy::VoicePolicy;
use super::port::TtsGateway;
use crate::feedback::usecase::FeedbackUseCase;
use crate::question::usecase::QuestionUseCase;
use crate::session::usecase::SessionUseCase;
use crate::shared::port::{AsyncExecutor, MetricsPort};
use crate::stt::usecase::SttUseCase;

/// 이벤트 재전송 버퍼에 남기는 이벤트 수의 기본값.
pub const DEFAULT_EVENT_REPLAY_BUFFER_LIMIT: usize = 256;

/// 지금 시각(Unix epoch 밀리초). Java `LongSupplier nowMillisSupplier`.
pub type NowMillis = Arc<dyn Fn() -> i64 + Send + Sync>;

/// 실제 시계.
pub fn system_now_millis() -> NowMillis {
    Arc::new(|| Utc::now().timestamp_millis())
}

/// Java 생성자의 인자들.
#[derive(Clone)]
pub struct VoiceSessionDependencies {
    pub stt_use_case: Arc<SttUseCase>,
    pub feedback_use_case: Arc<FeedbackUseCase>,
    pub session_use_case: Arc<SessionUseCase>,
    pub question_use_case: Arc<QuestionUseCase>,
    pub tts_gateway: Arc<dyn TtsGateway>,
    pub async_executor: Arc<dyn AsyncExecutor>,
    pub voice_policy: Arc<dyn VoicePolicy>,
    pub metrics: Arc<dyn MetricsPort>,
}

/// 복구 조회의 현재 질문.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryQuestion {
    pub id: Option<String>,
    pub text: Option<String>,
    pub group: Option<String>,
    pub group_id: Option<String>,
    pub question_type: Option<String>,
}

/// 끊긴 클라이언트가 다시 붙을 때 보는 세션 상태.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoverySnapshot {
    pub session_id: String,
    pub voice_session_id: String,
    pub active: bool,
    pub stopped: bool,
    /// 정지하지 않았으면 빈 문자열.
    pub stop_reason: String,
    pub current_turn_id: i64,
    pub current_question: Option<RecoveryQuestion>,
    pub turn_processing: bool,
    pub has_buffered_audio: bool,
    pub last_accepted_chunk_sequence: Option<i64>,
    pub latest_event_id: i64,
    /// 이 id 뒤의 이벤트부터 다시 받으면 된다.
    pub replay_from_event_id: i64,
    /// 재전송 버퍼가 잘려서 `last_seen_event_id` 뒤의 이벤트 일부를 다시 보낼 수 없다.
    pub gap_detected: bool,
}

pub struct VoiceSessionUseCase {
    facade: Arc<VoiceSessionFacade>,
}

impl VoiceSessionUseCase {
    /// 재전송 버퍼 256개, 실제 시계.
    pub fn new(deps: VoiceSessionDependencies) -> Self {
        Self::with_event_replay_buffer_limit(deps, DEFAULT_EVENT_REPLAY_BUFFER_LIMIT)
    }

    /// `event_replay_buffer_limit`은 1보다 작으면 1이다.
    pub fn with_event_replay_buffer_limit(
        deps: VoiceSessionDependencies,
        event_replay_buffer_limit: usize,
    ) -> Self {
        Self::with_clock(deps, event_replay_buffer_limit, system_now_millis())
    }

    pub fn with_clock(
        deps: VoiceSessionDependencies,
        event_replay_buffer_limit: usize,
        now_millis: NowMillis,
    ) -> Self {
        Self {
            facade: Arc::new(VoiceSessionFacade::new(
                deps,
                event_replay_buffer_limit,
                now_millis,
            )),
        }
    }

    /// 세션을 열고 음성 세션 id를 돌려준다. 이벤트는 싱크를 등록해야 시작된다.
    pub fn open(&self, command: &VoiceSessionOpenCommand) -> AppResult<String> {
        self.facade.open(command)
    }

    pub fn register_sink(
        &self,
        voice_session_id: &str,
        sink: Arc<dyn VoiceEventSink>,
    ) -> AppResult<()> {
        self.facade.register_sink(voice_session_id, sink, None)
    }

    /// `since_event_id` 뒤의 이벤트를 먼저 다시 보내고 싱크를 등록한다.
    pub fn register_sink_since(
        &self,
        voice_session_id: &str,
        sink: Arc<dyn VoiceEventSink>,
        since_event_id: Option<i64>,
    ) -> AppResult<()> {
        self.facade
            .register_sink(voice_session_id, sink, since_event_id)
    }

    /// 등록할 때 넘긴 것과 같은 `Arc`로 찾는다. 모르는 세션이면 아무것도 하지 않는다.
    pub fn unregister_sink(&self, voice_session_id: &str, sink: &Arc<dyn VoiceEventSink>) {
        self.facade.unregister_sink(voice_session_id, sink)
    }

    pub fn recover(
        &self,
        voice_session_id: &str,
        last_seen_event_id: Option<i64>,
    ) -> AppResult<RecoverySnapshot> {
        self.facade.recover(voice_session_id, last_seen_event_id)
    }

    pub fn append_audio(&self, command: &VoiceAudioChunkCommand) -> AppResult<()> {
        self.facade.append_audio(command)
    }

    pub fn complete_answer(&self, command: &VoiceAnswerCommand) -> AppResult<()> {
        self.facade.complete_answer(command)
    }

    /// 모르는 세션이면 아무것도 하지 않는다.
    pub fn stop(&self, command: &VoiceSessionStopCommand) -> AppResult<()> {
        self.facade.stop(command)
    }
}
