//! 세션을 열 때 정해지는 모델과 제공자 (Java `VoiceSessionFacade.RuntimeSettings`).

use omypic_domain::error::AppResult;
use omypic_domain::shared::text_utils::java_is_blank;

use crate::feedback::policy::FeedbackModelPolicy;
use crate::voice::model::VoiceSessionOpenCommand;
use crate::voice::policy::{VoiceModelCatalog, VoicePolicy};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RuntimeSettings {
    pub(super) stt_model: String,
    pub(super) feedback_model: String,
    pub(super) feedback_provider: String,
    pub(super) ai_enabled: bool,
    pub(super) feedback_language: String,
    pub(super) tts_model: String,
    pub(super) tts_voice: String,
}

impl RuntimeSettings {
    /// 명령의 값이 비었으면 정책의 기본값을 쓴다. 피드백 제공자를 주지 않으면 모델에서 정한다.
    pub(super) fn resolve(
        command: &VoiceSessionOpenCommand,
        policy: &dyn VoicePolicy,
    ) -> AppResult<Self> {
        let configured_model = policy.voice_feedback_model();
        let requested_model = command.feedback_model.as_deref();
        let feedback_provider = resolve_feedback_provider(
            command.feedback_provider.as_deref(),
            &first_non_blank(requested_model, &configured_model),
        )?;
        let feedback_model = match requested_model.filter(|model| !java_is_blank(model)) {
            Some(model) => model.to_string(),
            None => default_feedback_model(feedback_provider, &configured_model)?,
        };
        Ok(Self {
            stt_model: first_non_blank(command.stt_model.as_deref(), &policy.voice_stt_model()),
            feedback_model,
            feedback_provider: feedback_provider.to_string(),
            ai_enabled: is_ai_enabled(command),
            feedback_language: first_non_blank(
                command.feedback_language.as_deref(),
                &policy.voice_feedback_language(),
            ),
            tts_model: first_non_blank(command.tts_model.as_deref(), &policy.voice_tts_model()),
            tts_voice: first_non_blank(command.tts_voice.as_deref(), &policy.voice_tts_voice()),
        })
    }

    /// 서버는 AI가 켜졌고 호스팅 STT 모델일 때만 음성 인식 API를 부른다.
    pub(super) fn server_stt(&self) -> bool {
        self.ai_enabled && !VoiceModelCatalog::is_local_stt(Some(&self.stt_model))
    }

    /// 서버는 AI가 켜졌고 호스팅 TTS 모델일 때만 음성 합성 API를 부른다.
    pub(super) fn server_tts(&self) -> bool {
        self.ai_enabled && !VoiceModelCatalog::is_client_tts(Some(&self.tts_model))
    }

    pub(super) fn uses_anthropic_feedback(&self) -> bool {
        self.ai_enabled && self.feedback_provider == FeedbackModelPolicy::ANTHROPIC_PROVIDER
    }

    pub(super) fn needs_open_ai_key(&self) -> bool {
        self.server_stt()
            || self.server_tts()
            || (self.ai_enabled && !self.uses_anthropic_feedback())
    }
}

/// `ai_enabled`가 없으면 켜진 것으로 본다(기존 클라이언트 호환).
pub(super) fn is_ai_enabled(command: &VoiceSessionOpenCommand) -> bool {
    command.ai_enabled.unwrap_or(true)
}

fn default_feedback_model(provider: &str, configured_model: &str) -> AppResult<String> {
    if provider == FeedbackModelPolicy::provider_of(Some(configured_model)) {
        return Ok(configured_model.to_string());
    }
    Ok(FeedbackModelPolicy::default_model(Some(provider))?.to_string())
}

fn resolve_feedback_provider(
    requested_provider: Option<&str>,
    feedback_model: &str,
) -> AppResult<&'static str> {
    match requested_provider.filter(|provider| !java_is_blank(provider)) {
        Some(provider) => FeedbackModelPolicy::normalize_provider(Some(provider)),
        None => Ok(FeedbackModelPolicy::provider_of(Some(feedback_model))),
    }
}

fn first_non_blank(value: Option<&str>, fallback: &str) -> String {
    match value.filter(|value| !java_is_blank(value)) {
        Some(value) => value.to_string(),
        None => fallback.to_string(),
    }
}
