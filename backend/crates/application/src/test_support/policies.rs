use crate::feedback::policy::FeedbackPolicy;
use crate::rulebook::policy::RagPolicy;
use crate::shared::policy::DataDirProvider;
use crate::stt::model::VadSettings;
use crate::stt::policy::SttPolicy;
use crate::voice::policy::VoicePolicy;

/// 필드를 그대로 돌려주는 정책. 테스트가 필요한 값만 바꿔 쓴다.
#[derive(Debug, Clone)]
pub struct FakeFeedbackPolicy {
    pub summary_max_chars: usize,
    pub example_min_ratio: f64,
    pub example_max_ratio: f64,
    pub wrongnote_summary_max_chars: usize,
    pub wrongnote_window_size: usize,
}

impl Default for FakeFeedbackPolicy {
    fn default() -> Self {
        Self {
            summary_max_chars: 255,
            example_min_ratio: 0.8,
            example_max_ratio: 1.2,
            wrongnote_summary_max_chars: 255,
            wrongnote_window_size: 100,
        }
    }
}

impl FeedbackPolicy for FakeFeedbackPolicy {
    fn get_summary_max_chars(&self) -> usize {
        self.summary_max_chars
    }
    fn get_example_min_ratio(&self) -> f64 {
        self.example_min_ratio
    }
    fn get_example_max_ratio(&self) -> f64 {
        self.example_max_ratio
    }
    fn get_wrongnote_summary_max_chars(&self) -> usize {
        self.wrongnote_summary_max_chars
    }
    fn get_wrongnote_window_size(&self) -> usize {
        self.wrongnote_window_size
    }
}

#[derive(Debug, Clone)]
pub struct FakeRagPolicy {
    pub max_context_chunks: usize,
}

impl Default for FakeRagPolicy {
    fn default() -> Self {
        Self {
            max_context_chunks: 3,
        }
    }
}

impl RagPolicy for FakeRagPolicy {
    fn get_max_context_chunks(&self) -> usize {
        self.max_context_chunks
    }
}

#[derive(Debug, Clone)]
pub struct FakeSttPolicy {
    pub max_file_bytes: u64,
    pub retry_max: u32,
    pub vad_settings: VadSettings,
}

impl Default for FakeSttPolicy {
    fn default() -> Self {
        Self {
            max_file_bytes: 100,
            retry_max: 2,
            vad_settings: VadSettings {
                prefix_padding_ms: 300,
                silence_duration_ms: 500,
                threshold: 0.6,
            },
        }
    }
}

impl SttPolicy for FakeSttPolicy {
    fn get_max_file_bytes(&self) -> u64 {
        self.max_file_bytes
    }
    fn retry_max(&self) -> u32 {
        self.retry_max
    }
    fn get_vad_settings(&self) -> VadSettings {
        self.vad_settings
    }
}

#[derive(Debug, Clone)]
pub struct FakeVoicePolicy {
    pub stt_model: String,
    pub feedback_model: String,
    pub feedback_language: String,
    pub tts_model: String,
    pub tts_voice: String,
    pub recovery_retention_ms: i64,
    pub stopped_context_max: usize,
}

impl Default for FakeVoicePolicy {
    fn default() -> Self {
        Self {
            stt_model: "gpt-4o-mini-transcribe".to_string(),
            feedback_model: "gpt-5-mini".to_string(),
            feedback_language: "ko".to_string(),
            tts_model: "gpt-4o-mini-tts".to_string(),
            tts_voice: "alloy".to_string(),
            recovery_retention_ms: 300_000,
            stopped_context_max: 100,
        }
    }
}

impl VoicePolicy for FakeVoicePolicy {
    fn voice_stt_model(&self) -> String {
        self.stt_model.clone()
    }
    fn voice_feedback_model(&self) -> String {
        self.feedback_model.clone()
    }
    fn voice_feedback_language(&self) -> String {
        self.feedback_language.clone()
    }
    fn voice_tts_model(&self) -> String {
        self.tts_model.clone()
    }
    fn voice_tts_voice(&self) -> String {
        self.tts_voice.clone()
    }
    fn voice_recovery_retention_ms(&self) -> i64 {
        self.recovery_retention_ms
    }
    fn voice_stopped_context_max(&self) -> usize {
        self.stopped_context_max
    }
}

#[derive(Debug, Clone)]
pub struct FakeDataDirProvider {
    pub data_dir: String,
}

impl FakeDataDirProvider {
    pub fn new(data_dir: &str) -> Self {
        Self {
            data_dir: data_dir.to_string(),
        }
    }
}

impl DataDirProvider for FakeDataDirProvider {
    fn get_data_dir(&self) -> String {
        self.data_dir.clone()
    }
}
