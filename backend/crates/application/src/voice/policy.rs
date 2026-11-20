use omypic_domain::shared::text_utils::java_trim;

pub trait VoicePolicy: Send + Sync {
    fn voice_stt_model(&self) -> String;
    fn voice_feedback_model(&self) -> String;
    fn voice_feedback_language(&self) -> String;
    fn voice_tts_model(&self) -> String;
    fn voice_tts_voice(&self) -> String;
    fn voice_recovery_retention_ms(&self) -> i64;
    fn voice_stopped_context_max(&self) -> usize;
    /// 이벤트 스트림이 붙어 있지 않은 진행 중 세션을 멈추기까지 기다리는 시간. 0 이하이면 기본값.
    fn voice_idle_timeout_ms(&self) -> i64;
}

/// 지원하는 음성 모델 목록.
pub struct VoiceModelCatalog;

const STT_MODELS: [&str; 4] = [
    "gpt-4o-transcribe",
    "gpt-4o-mini-transcribe",
    "whisper-1",
    VoiceModelCatalog::LOCAL_STT_MODEL,
];
const TTS_MODELS: [&str; 4] = [
    "gpt-4o-mini-tts",
    "tts-1-hd",
    "tts-1",
    VoiceModelCatalog::LOCAL_TTS_MODEL,
];
/// 0.2.x 클라이언트가 저장해 둔 기기 내장 음성. 더는 목록에 없고 `local-kokoro`로 읽는다.
const LEGACY_DEVICE_TTS_MODEL: &str = "device-voice";
const TTS_VOICES: [&str; 5] = ["alloy", "echo", "fable", "nova", "shimmer"];
const FEEDBACK_LANGUAGES: [&str; 2] = ["ko", "en"];

impl VoiceModelCatalog {
    /// 학습자 기기에서 도는 음성 인식(API도 비용도 없다). 클라이언트가 전사해서 글자만 보낸다.
    pub const LOCAL_STT_MODEL: &'static str = "local-whisper";
    /// 학습자 기기에서 도는 모델의 음성 합성. 서버는 오디오 대신 읽을 글자를 보낸다.
    pub const LOCAL_TTS_MODEL: &'static str = "local-kokoro";

    pub fn is_local_stt(model: Option<&str>) -> bool {
        equals_ignore_case(model, Self::LOCAL_STT_MODEL)
    }

    /// 클라이언트가 직접 읽는 TTS 모델이면 `true`. 0.2.x 설정이 보내는 `device-voice`도 포함한다.
    pub fn is_client_tts(model: Option<&str>) -> bool {
        equals_ignore_case(model, LEGACY_DEVICE_TTS_MODEL)
            || equals_ignore_case(model, Self::LOCAL_TTS_MODEL)
    }

    /// 목록에서 빠진 `device-voice`를 `local-kokoro`로 바꿔 돌려준다. 다른 값은 그대로다.
    pub fn normalize_tts_model(model: &str) -> String {
        if equals_ignore_case(Some(model), LEGACY_DEVICE_TTS_MODEL) {
            Self::LOCAL_TTS_MODEL.to_string()
        } else {
            model.to_string()
        }
    }

    pub fn supported_stt_models() -> &'static [&'static str] {
        &STT_MODELS
    }

    pub fn supported_tts_models() -> &'static [&'static str] {
        &TTS_MODELS
    }

    pub fn supported_voices() -> &'static [&'static str] {
        &TTS_VOICES
    }

    pub fn supported_feedback_languages() -> &'static [&'static str] {
        &FEEDBACK_LANGUAGES
    }
}

fn equals_ignore_case(model: Option<&str>, expected: &str) -> bool {
    java_trim(model.unwrap_or("")).to_lowercase() == expected
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_local_stt_ignores_case_and_surrounding_spaces() {
        assert!(VoiceModelCatalog::is_local_stt(Some(" Local-Whisper ")));
        assert!(!VoiceModelCatalog::is_local_stt(Some("whisper-1")));
        assert!(!VoiceModelCatalog::is_local_stt(None));
    }

    #[test]
    fn is_client_tts_covers_legacy_device_voice_and_local_models() {
        assert!(VoiceModelCatalog::is_client_tts(Some("device-voice")));
        assert!(VoiceModelCatalog::is_client_tts(Some("LOCAL-KOKORO")));
        assert!(!VoiceModelCatalog::is_client_tts(Some("tts-1")));
    }

    #[test]
    fn tts_models_do_not_list_device_voice() {
        assert_eq!(
            ["gpt-4o-mini-tts", "tts-1-hd", "tts-1", "local-kokoro"],
            VoiceModelCatalog::supported_tts_models()
        );
    }

    #[test]
    fn normalize_tts_model_maps_only_device_voice() {
        assert_eq!(
            "local-kokoro",
            VoiceModelCatalog::normalize_tts_model(" Device-Voice ")
        );
        assert_eq!("tts-1", VoiceModelCatalog::normalize_tts_model("tts-1"));
    }

    #[test]
    fn catalog_lists_match_java() {
        assert_eq!(4, VoiceModelCatalog::supported_stt_models().len());
        assert_eq!(4, VoiceModelCatalog::supported_tts_models().len());
        assert_eq!(
            ["alloy", "echo", "fable", "nova", "shimmer"],
            VoiceModelCatalog::supported_voices()
        );
        assert_eq!(
            ["ko", "en"],
            VoiceModelCatalog::supported_feedback_languages()
        );
    }
}
