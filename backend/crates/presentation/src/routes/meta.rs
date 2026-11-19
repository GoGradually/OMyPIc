use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use omypic_application::feedback::policy::FeedbackModelPolicy;
use omypic_application::voice::policy::VoiceModelCatalog;
use omypic_domain::shared::text_utils::java_is_blank;
use serde::Serialize;

use crate::error::ApiError;
use crate::state::AppState;

pub(super) fn routes() -> Router<AppState> {
    Router::new().route("/api/meta/models", get(models))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ModelMetaResponse {
    feedback_models: Vec<&'static str>,
    default_feedback_model: String,
    feedback_providers: Vec<FeedbackProviderMeta>,
    voice_stt_models: Vec<&'static str>,
    default_voice_stt_model: String,
    tts_models: Vec<&'static str>,
    default_tts_model: String,
    voices: Vec<&'static str>,
    default_voice: String,
    feedback_languages: Vec<&'static str>,
    default_feedback_language: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FeedbackProviderMeta {
    provider: &'static str,
    models: Vec<&'static str>,
    default_model: &'static str,
}

/// 설정값이 비어 있거나 지원 목록에 없으면 목록의 첫 값이다. 목록이 비면 빈 문자열이다.
fn resolve_default(configured: &str, supported: &[&str]) -> String {
    let Some(first) = supported.first() else {
        return String::new();
    };
    if java_is_blank(configured) || !supported.contains(&configured) {
        return (*first).to_string();
    }
    configured.to_string()
}

async fn models(State(state): State<AppState>) -> Result<Json<ModelMetaResponse>, ApiError> {
    let policy = &state.voice_policy;
    let feedback_models = FeedbackModelPolicy::supported_models();
    let stt_models = VoiceModelCatalog::supported_stt_models();
    let tts_models = VoiceModelCatalog::supported_tts_models();
    let voices = VoiceModelCatalog::supported_voices();
    let languages = VoiceModelCatalog::supported_feedback_languages();
    let configured_feedback_model = policy.voice_feedback_model();

    let mut providers = Vec::new();
    for provider in FeedbackModelPolicy::supported_providers() {
        let models = FeedbackModelPolicy::supported_models_for(Some(provider))?;
        let default_model = if models.contains(&configured_feedback_model.as_str()) {
            models
                .iter()
                .copied()
                .find(|model| *model == configured_feedback_model)
                .unwrap_or_default()
        } else {
            FeedbackModelPolicy::default_model(Some(provider))?
        };
        providers.push(FeedbackProviderMeta {
            provider,
            models: models.to_vec(),
            default_model,
        });
    }

    Ok(Json(ModelMetaResponse {
        feedback_models: feedback_models.to_vec(),
        default_feedback_model: resolve_default(&configured_feedback_model, feedback_models),
        feedback_providers: providers,
        voice_stt_models: stt_models.to_vec(),
        default_voice_stt_model: resolve_default(&policy.voice_stt_model(), stt_models),
        tts_models: tts_models.to_vec(),
        default_tts_model: resolve_default(&policy.voice_tts_model(), tts_models),
        voices: voices.to_vec(),
        default_voice: resolve_default(&policy.voice_tts_voice(), voices),
        feedback_languages: languages.to_vec(),
        default_feedback_language: resolve_default(&policy.voice_feedback_language(), languages),
    }))
}
