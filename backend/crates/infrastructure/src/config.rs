//! `AppProperties`에 대응하는 설정. 기본값과 `OMYPIC_*` 환경 변수 이름은 `application.yml`과 같다.
use std::str::FromStr;

use omypic_application::feedback::policy::FeedbackPolicy;
use omypic_application::rulebook::policy::RagPolicy;
use omypic_application::shared::policy::DataDirProvider;
use omypic_application::stt::model::VadSettings;
use omypic_application::stt::policy::SttPolicy;
use omypic_application::voice::policy::VoicePolicy;
use omypic_domain::error::{AppError, AppResult};

use crate::storage::default_data_dir;

#[derive(Debug, Clone, PartialEq)]
pub struct AppConfig {
    /// `OMYPIC_PORT`, 기본 4317. 서버는 항상 `127.0.0.1`에만 바인딩한다.
    pub port: u16,
    /// `OMYPIC_DATA_DIR`, 기본 `<홈>/OMyPIc`.
    pub data_dir: String,
    /// `OMYPIC_ALLOWED_ORIGINS`(쉼표로 구분, 기본 목록을 대신한다). 브라우저가 보내는 `Origin`이
    /// 이 목록에 없으면 요청을 거부한다. 기본값은 Vite 개발 서버와 Tauri 웹뷰의 origin이다.
    pub allowed_origins: Vec<String>,
    pub stt: SttConfig,
    pub rag: RagConfig,
    pub feedback: FeedbackConfig,
    pub voice: VoiceConfig,
    pub integrations: IntegrationsConfig,
    pub local_models: LocalModelsConfig,
}

/// 온디바이스 음성 모델 파일을 받아 오는 곳과 받을 수 있는 저장소.
#[derive(Debug, Clone, PartialEq)]
pub struct LocalModelsConfig {
    /// `OMYPIC_LOCAL_MODEL_UPSTREAM`, 기본 `https://huggingface.co`.
    pub upstream: String,
    /// `OMYPIC_LOCAL_MODEL_REPOS`(쉼표로 구분, 기본 목록을 대신한다). `org/name` 꼴이고 대소문자를 가린다.
    pub allowed_repos: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SttConfig {
    pub max_file_bytes: u64,
    pub max_duration_seconds: u32,
    pub retry_max: u32,
    pub vad: VadConfig,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VadConfig {
    pub prefix_padding_ms: i32,
    pub silence_duration_ms: i32,
    pub threshold: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RagConfig {
    pub provider: String,
    pub embedding_dim: usize,
    pub max_context_chunks: usize,
    pub model_path: String,
    pub model_version: String,
    pub model_sha256: String,
    pub download_url: String,
    pub download_timeout_seconds: u64,
    pub download_retry_max: u32,
    pub allow_hash_fallback: bool,
    pub model_max_vocab: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FeedbackConfig {
    pub summary_max_chars: usize,
    pub example_min_ratio: f64,
    pub example_max_ratio: f64,
    pub wrongnote_summary_max_chars: usize,
    pub wrongnote_window_size: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VoiceConfig {
    pub stt_model: String,
    pub feedback_model: String,
    pub feedback_language: String,
    pub tts_model: String,
    pub tts_voice: String,
    /// `OMYPIC_VOICE_RECOVERY_RETENTION_MS`, 기본 600000. 정지한 세션을 복구 조회용으로 두는 시간.
    pub recovery_retention_ms: i64,
    /// `OMYPIC_VOICE_STOPPED_CONTEXT_MAX`, 기본 1000. 보관하는 정지 세션의 최대 수.
    pub stopped_context_max: usize,
    /// `OMYPIC_VOICE_IDLE_TIMEOUT_MS`, 기본 600000. 이벤트 스트림이 끊긴 채 이만큼 아무 일도 없는
    /// 진행 중 세션은 `idle_timeout`으로 멈춘다(창을 닫았거나 앱이 죽은 클라이언트). 다시 붙는
    /// 클라이언트는 이 시간 안에 돌아오면 그대로 이어 간다.
    pub idle_timeout_ms: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IntegrationsConfig {
    pub openai: OpenAiConfig,
    pub anthropic: AnthropicConfig,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OpenAiConfig {
    pub base_url: String,
    pub responses_enabled: bool,
    pub conversation_rebase_turns: u32,
    /// `integrations.openai.timeout-seconds`(`OMYPIC_OPENAI_TIMEOUT_SECONDS`, 기본 300).
    /// LLM, STT, TTS 요청 하나의 전체 제한 시간. 1보다 작으면 1초로 본다.
    pub timeout_seconds: u64,
    pub logging: OpenAiLoggingConfig,
}

impl OpenAiConfig {
    /// 요청 하나의 전체 제한 시간(최소 1초).
    pub fn request_timeout(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.timeout_seconds.max(1))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OpenAiLoggingConfig {
    pub response_preview_chars: usize,
    pub full_body: bool,
    pub log_success_at_fine: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AnthropicConfig {
    pub base_url: String,
    pub max_tokens: u64,
    pub effort: String,
    pub timeout_seconds: u64,
    pub max_retries: u32,
}

/// `OMYPIC_ALLOWED_ORIGINS`가 없을 때 받아 주는 origin.
pub const DEFAULT_ALLOWED_ORIGINS: [&str; 5] = [
    "http://localhost:5173",
    "http://127.0.0.1:5173",
    "tauri://localhost",
    "http://tauri.localhost",
    "https://tauri.localhost",
];

/// `OMYPIC_LOCAL_MODEL_REPOS`가 없을 때 받아 주는 저장소.
pub const DEFAULT_LOCAL_MODEL_REPOS: [&str; 3] = [
    "Xenova/whisper-base.en",
    "onnx-community/Kokoro-82M-v1.0-ONNX",
    "onnx-community/Supertonic-TTS-2-ONNX",
];

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            port: 4317,
            data_dir: default_data_dir(),
            allowed_origins: DEFAULT_ALLOWED_ORIGINS
                .iter()
                .map(|origin| origin.to_string())
                .collect(),
            local_models: LocalModelsConfig {
                upstream: "https://huggingface.co".to_string(),
                allowed_repos: DEFAULT_LOCAL_MODEL_REPOS
                    .iter()
                    .map(|repo| repo.to_string())
                    .collect(),
            },
            stt: SttConfig {
                max_file_bytes: 26_214_400,
                max_duration_seconds: 180,
                retry_max: 2,
                vad: VadConfig {
                    prefix_padding_ms: 300,
                    silence_duration_ms: 200,
                    threshold: 0.5,
                },
            },
            rag: RagConfig {
                provider: "fasttext".to_string(),
                embedding_dim: 300,
                max_context_chunks: 4,
                model_path: String::new(),
                model_version: "cc.ko.300.vec.gz".to_string(),
                model_sha256: "9d71f0ae144e0f89dd233bfa7eca421be26bd1e5dd18e2cc56888a04be982d97"
                    .to_string(),
                download_url:
                    "https://dl.fbaipublicfiles.com/fasttext/vectors-crawl/cc.ko.300.vec.gz"
                        .to_string(),
                download_timeout_seconds: 1800,
                download_retry_max: 2,
                allow_hash_fallback: false,
                model_max_vocab: 200_000,
            },
            feedback: FeedbackConfig {
                summary_max_chars: 400,
                example_min_ratio: 0.8,
                example_max_ratio: 1.2,
                wrongnote_summary_max_chars: 255,
                wrongnote_window_size: 100,
            },
            voice: VoiceConfig {
                stt_model: "gpt-4o-mini-transcribe".to_string(),
                feedback_model: "gpt-5-nano".to_string(),
                feedback_language: "ko".to_string(),
                tts_model: "gpt-4o-mini-tts".to_string(),
                tts_voice: "alloy".to_string(),
                recovery_retention_ms: 600_000,
                stopped_context_max: 1000,
                idle_timeout_ms: 600_000,
            },
            integrations: IntegrationsConfig {
                openai: OpenAiConfig {
                    base_url: "https://api.openai.com".to_string(),
                    responses_enabled: true,
                    conversation_rebase_turns: 6,
                    timeout_seconds: 300,
                    logging: OpenAiLoggingConfig {
                        response_preview_chars: 1024,
                        full_body: false,
                        log_success_at_fine: true,
                    },
                },
                anthropic: AnthropicConfig {
                    base_url: "https://api.anthropic.com".to_string(),
                    max_tokens: 16_000,
                    effort: "low".to_string(),
                    timeout_seconds: 120,
                    max_retries: 2,
                },
            },
        }
    }
}

impl AppConfig {
    /// 프로세스 환경 변수에서 읽는다.
    pub fn from_env() -> AppResult<Self> {
        Self::from_env_with(|name| std::env::var(name).ok())
    }

    /// `lookup`으로 환경 변수를 읽는다. 값이 없거나 비어 있으면 기본값을 쓰고,
    /// 값이 있는데 형식이 틀리면 변수 이름을 담은 `InvalidArgument`로 거절한다.
    pub fn from_env_with(lookup: impl Fn(&str) -> Option<String>) -> AppResult<Self> {
        let mut config = Self::default();
        let env = Env { lookup: &lookup };

        env.set("OMYPIC_PORT", &mut config.port)?;
        env.set("OMYPIC_DATA_DIR", &mut config.data_dir)?;
        if let Some(raw) = env.raw("OMYPIC_ALLOWED_ORIGINS") {
            config.allowed_origins = raw
                .split(',')
                .map(str::trim)
                .filter(|origin| !origin.is_empty())
                .map(|origin| {
                    if is_origin(origin) {
                        Ok(origin.to_string())
                    } else {
                        Err(invalid("OMYPIC_ALLOWED_ORIGINS", origin))
                    }
                })
                .collect::<AppResult<_>>()?;
        }

        env.set(
            "OMYPIC_LOCAL_MODEL_UPSTREAM",
            &mut config.local_models.upstream,
        )?;
        if let Some(raw) = env.raw("OMYPIC_LOCAL_MODEL_REPOS") {
            config.local_models.allowed_repos = raw
                .split(',')
                .map(str::trim)
                .filter(|repo| !repo.is_empty())
                .map(|repo| {
                    if is_repo(repo) {
                        Ok(repo.to_string())
                    } else {
                        Err(invalid("OMYPIC_LOCAL_MODEL_REPOS", repo))
                    }
                })
                .collect::<AppResult<_>>()?;
        }

        let rag = &mut config.rag;
        env.set("OMYPIC_RAG_PROVIDER", &mut rag.provider)?;
        env.set("OMYPIC_RAG_MODEL_PATH", &mut rag.model_path)?;
        env.set("OMYPIC_RAG_MODEL_VERSION", &mut rag.model_version)?;
        env.set("OMYPIC_RAG_MODEL_SHA256", &mut rag.model_sha256)?;
        env.set("OMYPIC_RAG_DOWNLOAD_URL", &mut rag.download_url)?;
        env.set_bool(
            "OMYPIC_RAG_ALLOW_HASH_FALLBACK",
            &mut rag.allow_hash_fallback,
        )?;

        let openai = &mut config.integrations.openai;
        let voice = &mut config.voice;
        env.set(
            "OMYPIC_VOICE_RECOVERY_RETENTION_MS",
            &mut voice.recovery_retention_ms,
        )?;
        env.set(
            "OMYPIC_VOICE_STOPPED_CONTEXT_MAX",
            &mut voice.stopped_context_max,
        )?;
        env.set("OMYPIC_VOICE_IDLE_TIMEOUT_MS", &mut voice.idle_timeout_ms)?;

        env.set("OMYPIC_OPENAI_BASE_URL", &mut openai.base_url)?;
        env.set_bool(
            "OMYPIC_OPENAI_RESPONSES_ENABLED",
            &mut openai.responses_enabled,
        )?;
        env.set(
            "OMYPIC_OPENAI_CONVERSATION_REBASE_TURNS",
            &mut openai.conversation_rebase_turns,
        )?;
        env.set("OMYPIC_OPENAI_TIMEOUT_SECONDS", &mut openai.timeout_seconds)?;
        env.set(
            "OMYPIC_OPENAI_LOG_RESPONSE_PREVIEW_CHARS",
            &mut openai.logging.response_preview_chars,
        )?;
        env.set_bool("OMYPIC_OPENAI_LOG_FULL_BODY", &mut openai.logging.full_body)?;
        env.set_bool(
            "OMYPIC_OPENAI_LOG_SUCCESS_AT_FINE",
            &mut openai.logging.log_success_at_fine,
        )?;

        let anthropic = &mut config.integrations.anthropic;
        env.set("OMYPIC_ANTHROPIC_BASE_URL", &mut anthropic.base_url)?;
        env.set("OMYPIC_ANTHROPIC_MAX_TOKENS", &mut anthropic.max_tokens)?;
        env.set("OMYPIC_ANTHROPIC_EFFORT", &mut anthropic.effort)?;
        env.set(
            "OMYPIC_ANTHROPIC_TIMEOUT_SECONDS",
            &mut anthropic.timeout_seconds,
        )?;
        env.set("OMYPIC_ANTHROPIC_MAX_RETRIES", &mut anthropic.max_retries)?;

        Ok(config)
    }
}

struct Env<'a> {
    lookup: &'a dyn Fn(&str) -> Option<String>,
}

impl Env<'_> {
    fn raw(&self, name: &str) -> Option<String> {
        (self.lookup)(name).filter(|value| !value.is_empty())
    }

    fn set<T: FromStr>(&self, name: &str, target: &mut T) -> AppResult<()> {
        if let Some(raw) = self.raw(name) {
            *target = raw.trim().parse().map_err(|_| invalid(name, &raw))?;
        }
        Ok(())
    }

    fn set_bool(&self, name: &str, target: &mut bool) -> AppResult<()> {
        if let Some(raw) = self.raw(name) {
            *target = match raw.trim().to_ascii_lowercase().as_str() {
                "true" | "1" | "yes" | "on" => true,
                "false" | "0" | "no" | "off" => false,
                _ => return Err(invalid(name, &raw)),
            };
        }
        Ok(())
    }
}

fn invalid(name: &str, value: &str) -> AppError {
    AppError::InvalidArgument(format!("Invalid value for {name}: '{value}'"))
}

/// `org/name` 꼴의 저장소 이름인지 본다(두 조각 모두 `[A-Za-z0-9._-]+`, `.`과 `..`은 안 된다).
fn is_repo(text: &str) -> bool {
    let mut parts = text.split('/');
    let (Some(org), Some(name), None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    [org, name].iter().all(|part| {
        !matches!(*part, "" | "." | "..")
            && part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    })
}

/// `scheme://host[:port]` 꼴의 origin인지 본다. 와일드카드, 경로, 쿼리, 끝의 `/`는 받지 않는다.
/// (CORS 계층은 목록에 `*`가 있으면 시작할 때 패닉하므로 설정 단계에서 거절한다.)
fn is_origin(text: &str) -> bool {
    let Some((scheme, authority)) = text.split_once("://") else {
        return false;
    };
    let mut scheme_chars = scheme.chars();
    let scheme_ok = scheme_chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && scheme_chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    let (host, port) = if let Some(rest) = authority.strip_prefix('[') {
        match rest.split_once(']') {
            Some((host, "")) => (host, None),
            Some((host, port)) => match port.strip_prefix(':') {
                Some(port) => (host, Some(port)),
                None => return false,
            },
            None => return false,
        }
    } else {
        match authority.split_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (authority, None),
        }
    };
    let host_ok = !host.is_empty()
        && if authority.starts_with('[') {
            host.chars().all(|c| c.is_ascii_hexdigit() || c == ':')
        } else {
            host.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_'))
        };
    let port_ok = port.is_none_or(|port| {
        !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()) && port.parse::<u16>().is_ok()
    });
    scheme_ok && host_ok && port_ok
}

impl DataDirProvider for AppConfig {
    fn get_data_dir(&self) -> String {
        self.data_dir.clone()
    }
}

impl SttPolicy for AppConfig {
    fn get_max_file_bytes(&self) -> u64 {
        self.stt.max_file_bytes
    }

    fn retry_max(&self) -> u32 {
        self.stt.retry_max
    }

    fn get_vad_settings(&self) -> VadSettings {
        VadSettings {
            prefix_padding_ms: self.stt.vad.prefix_padding_ms,
            silence_duration_ms: self.stt.vad.silence_duration_ms,
            threshold: self.stt.vad.threshold,
        }
    }
}

impl RagPolicy for AppConfig {
    fn get_max_context_chunks(&self) -> usize {
        self.rag.max_context_chunks
    }
}

impl FeedbackPolicy for AppConfig {
    fn get_summary_max_chars(&self) -> usize {
        self.feedback.summary_max_chars
    }

    fn get_example_min_ratio(&self) -> f64 {
        self.feedback.example_min_ratio
    }

    fn get_example_max_ratio(&self) -> f64 {
        self.feedback.example_max_ratio
    }

    fn get_wrongnote_summary_max_chars(&self) -> usize {
        self.feedback.wrongnote_summary_max_chars
    }

    fn get_wrongnote_window_size(&self) -> usize {
        self.feedback.wrongnote_window_size
    }
}

impl VoicePolicy for AppConfig {
    fn voice_stt_model(&self) -> String {
        self.voice.stt_model.clone()
    }

    fn voice_feedback_model(&self) -> String {
        self.voice.feedback_model.clone()
    }

    fn voice_feedback_language(&self) -> String {
        self.voice.feedback_language.clone()
    }

    fn voice_tts_model(&self) -> String {
        self.voice.tts_model.clone()
    }

    fn voice_tts_voice(&self) -> String {
        self.voice.tts_voice.clone()
    }

    fn voice_recovery_retention_ms(&self) -> i64 {
        self.voice.recovery_retention_ms
    }

    fn voice_stopped_context_max(&self) -> usize {
        self.voice.stopped_context_max
    }

    fn voice_idle_timeout_ms(&self) -> i64 {
        self.voice.idle_timeout_ms
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn from_map(pairs: &[(&str, &str)]) -> AppResult<AppConfig> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        AppConfig::from_env_with(|name| map.get(name).cloned())
    }

    #[test]
    fn policy_getters_delegate_to_nested_properties() {
        let mut config = AppConfig {
            data_dir: "/tmp/omypic".to_string(),
            ..AppConfig::default()
        };
        config.stt.max_file_bytes = 1234;
        config.stt.retry_max = 7;
        config.stt.vad.prefix_padding_ms = 111;
        config.stt.vad.silence_duration_ms = 222;
        config.stt.vad.threshold = 0.8;
        config.rag.max_context_chunks = 9;
        config.feedback.summary_max_chars = 333;
        config.feedback.example_min_ratio = 0.7;
        config.feedback.example_max_ratio = 1.5;
        config.feedback.wrongnote_summary_max_chars = 88;
        config.feedback.wrongnote_window_size = 33;
        config.voice.feedback_model = "gpt-4o-mini".to_string();

        assert_eq!("/tmp/omypic", config.get_data_dir());
        assert_eq!(1234, config.get_max_file_bytes());
        assert_eq!(7, config.retry_max());
        let vad = config.get_vad_settings();
        assert_eq!(111, vad.prefix_padding_ms);
        assert_eq!(222, vad.silence_duration_ms);
        assert_eq!(0.8, vad.threshold);
        assert_eq!(9, config.get_max_context_chunks());
        assert_eq!(333, config.get_summary_max_chars());
        assert_eq!(0.7, config.get_example_min_ratio());
        assert_eq!(1.5, config.get_example_max_ratio());
        assert_eq!(88, config.get_wrongnote_summary_max_chars());
        assert_eq!(33, config.get_wrongnote_window_size());
        assert_eq!("gpt-4o-mini-transcribe", config.voice_stt_model());
        assert_eq!("gpt-4o-mini", config.voice_feedback_model());
        assert_eq!("ko", config.voice_feedback_language());
        assert_eq!("gpt-4o-mini-tts", config.voice_tts_model());
        assert_eq!("alloy", config.voice_tts_voice());
        assert_eq!(600_000, config.voice_recovery_retention_ms());
        assert_eq!(1000, config.voice_stopped_context_max());
        assert_eq!(600_000, config.voice_idle_timeout_ms());
    }

    #[test]
    fn voice_session_lifetimes_can_be_set_by_env() {
        let config = from_map(&[
            ("OMYPIC_VOICE_IDLE_TIMEOUT_MS", "2000"),
            ("OMYPIC_VOICE_RECOVERY_RETENTION_MS", "3000"),
            ("OMYPIC_VOICE_STOPPED_CONTEXT_MAX", "7"),
        ])
        .unwrap();
        assert_eq!(2000, config.voice_idle_timeout_ms());
        assert_eq!(3000, config.voice_recovery_retention_ms());
        assert_eq!(7, config.voice_stopped_context_max());
        assert!(from_map(&[("OMYPIC_VOICE_IDLE_TIMEOUT_MS", "soon")]).is_err());
    }

    #[test]
    fn openai_request_timeout_defaults_to_300_seconds_and_is_at_least_one_second() {
        let config = AppConfig::default();
        assert_eq!(300, config.integrations.openai.timeout_seconds);
        assert_eq!(
            std::time::Duration::from_secs(300),
            config.integrations.openai.request_timeout()
        );

        let config = from_map(&[("OMYPIC_OPENAI_TIMEOUT_SECONDS", "45")]).unwrap();
        assert_eq!(
            std::time::Duration::from_secs(45),
            config.integrations.openai.request_timeout()
        );

        let config = from_map(&[("OMYPIC_OPENAI_TIMEOUT_SECONDS", "0")]).unwrap();
        assert_eq!(
            std::time::Duration::from_secs(1),
            config.integrations.openai.request_timeout()
        );

        assert!(from_map(&[("OMYPIC_OPENAI_TIMEOUT_SECONDS", "soon")]).is_err());
    }

    #[test]
    fn allowed_origins_default_to_vite_dev_server_and_tauri_webview() {
        assert_eq!(
            vec![
                "http://localhost:5173",
                "http://127.0.0.1:5173",
                "tauri://localhost",
                "http://tauri.localhost",
                "https://tauri.localhost",
            ],
            AppConfig::default().allowed_origins
        );
    }

    #[test]
    fn allowed_origins_env_replaces_default_list() {
        let config = from_map(&[(
            "OMYPIC_ALLOWED_ORIGINS",
            " http://localhost:3000 ,tauri://localhost,, ",
        )])
        .unwrap();

        assert_eq!(
            vec!["http://localhost:3000", "tauri://localhost"],
            config.allowed_origins
        );
    }

    #[test]
    fn allowed_origins_accept_scheme_host_and_optional_port() {
        let config = from_map(&[(
            "OMYPIC_ALLOWED_ORIGINS",
            "http://localhost:5173,tauri://localhost,https://app.example.com,http://[::1]:8080",
        )])
        .unwrap();

        assert_eq!(4, config.allowed_origins.len());
    }

    #[test]
    fn allowed_origins_reject_wildcards_and_non_origins_naming_the_variable() {
        for value in [
            "*",
            "http://localhost:5173,*",
            "https://*.example.com",
            "localhost:5173",
            "http://localhost:5173/",
            "http://localhost/path",
            "http://",
            "://localhost",
            "http://localhost:",
            "http://localhost:99999",
            "http://local host",
            "http://[::1",
        ] {
            let error = from_map(&[("OMYPIC_ALLOWED_ORIGINS", value)]).unwrap_err();

            match error {
                AppError::InvalidArgument(message) => {
                    assert!(message.contains("OMYPIC_ALLOWED_ORIGINS"), "{message}");
                }
                other => panic!("expected InvalidArgument for {value}, got {other:?}"),
            }
        }
    }

    #[test]
    fn default_allowed_repos_include_korean_model() {
        let config = AppConfig::default();
        assert!(config
            .local_models
            .allowed_repos
            .contains(&"onnx-community/Supertonic-TTS-2-ONNX".to_string()));
    }

    #[test]
    fn local_model_settings_default_and_can_be_overridden() {
        let config = AppConfig::default();
        assert_eq!("https://huggingface.co", config.local_models.upstream);
        assert_eq!(
            vec![
                "Xenova/whisper-base.en",
                "onnx-community/Kokoro-82M-v1.0-ONNX",
                "onnx-community/Supertonic-TTS-2-ONNX"
            ],
            config.local_models.allowed_repos
        );

        let config = from_map(&[
            ("OMYPIC_LOCAL_MODEL_UPSTREAM", "http://mirror.local:9000"),
            ("OMYPIC_LOCAL_MODEL_REPOS", " a/b , c/d.e,, "),
        ])
        .unwrap();
        assert_eq!("http://mirror.local:9000", config.local_models.upstream);
        assert_eq!(vec!["a/b", "c/d.e"], config.local_models.allowed_repos);
    }

    #[test]
    fn local_model_repos_must_be_org_slash_name() {
        for value in ["justname", "a/b/c", "a/..", "/b", "a/b c", "*", "a/b,x"] {
            let error = from_map(&[("OMYPIC_LOCAL_MODEL_REPOS", value)]).unwrap_err();

            match error {
                AppError::InvalidArgument(message) => {
                    assert!(message.contains("OMYPIC_LOCAL_MODEL_REPOS"), "{message}");
                }
                other => panic!("expected InvalidArgument for {value}, got {other:?}"),
            }
        }
    }

    #[test]
    fn defaults_match_application_yml() {
        let config = AppConfig::default();

        assert_eq!(4317, config.port);
        assert!(config.data_dir.ends_with("OMyPIc"));
        assert_eq!(26_214_400, config.stt.max_file_bytes);
        assert_eq!(180, config.stt.max_duration_seconds);
        assert_eq!(2, config.stt.retry_max);
        assert_eq!(300, config.stt.vad.prefix_padding_ms);
        assert_eq!(200, config.stt.vad.silence_duration_ms);
        assert_eq!(0.5, config.stt.vad.threshold);
        assert_eq!("fasttext", config.rag.provider);
        assert_eq!(300, config.rag.embedding_dim);
        assert_eq!(4, config.rag.max_context_chunks);
        assert_eq!("", config.rag.model_path);
        assert_eq!("cc.ko.300.vec.gz", config.rag.model_version);
        assert_eq!(1800, config.rag.download_timeout_seconds);
        assert_eq!(2, config.rag.download_retry_max);
        assert!(!config.rag.allow_hash_fallback);
        assert_eq!(200_000, config.rag.model_max_vocab);
        assert_eq!(400, config.feedback.summary_max_chars);
        assert_eq!(0.8, config.feedback.example_min_ratio);
        assert_eq!(1.2, config.feedback.example_max_ratio);
        assert_eq!(255, config.feedback.wrongnote_summary_max_chars);
        assert_eq!(100, config.feedback.wrongnote_window_size);
        assert_eq!("gpt-5-nano", config.voice.feedback_model);
        let openai = &config.integrations.openai;
        assert_eq!("https://api.openai.com", openai.base_url);
        assert!(openai.responses_enabled);
        assert_eq!(6, openai.conversation_rebase_turns);
        assert_eq!(1024, openai.logging.response_preview_chars);
        assert!(!openai.logging.full_body);
        assert!(openai.logging.log_success_at_fine);
        let anthropic = &config.integrations.anthropic;
        assert_eq!("https://api.anthropic.com", anthropic.base_url);
        assert_eq!(16_000, anthropic.max_tokens);
        assert_eq!("low", anthropic.effort);
        assert_eq!(120, anthropic.timeout_seconds);
        assert_eq!(2, anthropic.max_retries);
    }

    #[test]
    fn from_env_with_empty_map_returns_defaults() {
        assert_eq!(AppConfig::default(), from_map(&[]).unwrap());
    }

    #[test]
    fn from_env_with_overrides_values() {
        let config = from_map(&[
            ("OMYPIC_PORT", "8080"),
            ("OMYPIC_DATA_DIR", "/data/omypic"),
            ("OMYPIC_RAG_PROVIDER", "hash"),
            ("OMYPIC_RAG_MODEL_PATH", "/models/ko.vec.gz"),
            ("OMYPIC_RAG_MODEL_VERSION", "ko.vec.gz"),
            ("OMYPIC_RAG_MODEL_SHA256", "abc123"),
            ("OMYPIC_RAG_DOWNLOAD_URL", "https://example.com/ko.vec.gz"),
            ("OMYPIC_RAG_ALLOW_HASH_FALLBACK", "true"),
            ("OMYPIC_OPENAI_BASE_URL", "http://openai.local"),
            ("OMYPIC_OPENAI_RESPONSES_ENABLED", "false"),
            ("OMYPIC_OPENAI_CONVERSATION_REBASE_TURNS", "9"),
            ("OMYPIC_OPENAI_LOG_RESPONSE_PREVIEW_CHARS", "777"),
            ("OMYPIC_OPENAI_LOG_FULL_BODY", "TRUE"),
            ("OMYPIC_OPENAI_LOG_SUCCESS_AT_FINE", "false"),
            ("OMYPIC_ANTHROPIC_BASE_URL", "http://anthropic.local"),
            ("OMYPIC_ANTHROPIC_MAX_TOKENS", "4000"),
            ("OMYPIC_ANTHROPIC_EFFORT", "high"),
            ("OMYPIC_ANTHROPIC_TIMEOUT_SECONDS", "30"),
            ("OMYPIC_ANTHROPIC_MAX_RETRIES", "5"),
        ])
        .unwrap();

        assert_eq!(8080, config.port);
        assert_eq!("/data/omypic", config.data_dir);
        assert_eq!("hash", config.rag.provider);
        assert_eq!("/models/ko.vec.gz", config.rag.model_path);
        assert_eq!("ko.vec.gz", config.rag.model_version);
        assert_eq!("abc123", config.rag.model_sha256);
        assert_eq!("https://example.com/ko.vec.gz", config.rag.download_url);
        assert!(config.rag.allow_hash_fallback);
        let openai = &config.integrations.openai;
        assert_eq!("http://openai.local", openai.base_url);
        assert!(!openai.responses_enabled);
        assert_eq!(9, openai.conversation_rebase_turns);
        assert_eq!(777, openai.logging.response_preview_chars);
        assert!(openai.logging.full_body);
        assert!(!openai.logging.log_success_at_fine);
        let anthropic = &config.integrations.anthropic;
        assert_eq!("http://anthropic.local", anthropic.base_url);
        assert_eq!(4000, anthropic.max_tokens);
        assert_eq!("high", anthropic.effort);
        assert_eq!(30, anthropic.timeout_seconds);
        assert_eq!(5, anthropic.max_retries);
    }

    #[test]
    fn from_env_with_ignores_empty_values() {
        let config = from_map(&[("OMYPIC_PORT", ""), ("OMYPIC_RAG_PROVIDER", "")]).unwrap();

        assert_eq!(4317, config.port);
        assert_eq!("fasttext", config.rag.provider);
    }

    #[test]
    fn from_env_with_rejects_malformed_values_naming_the_variable() {
        for (name, value) in [
            ("OMYPIC_PORT", "abc"),
            ("OMYPIC_PORT", "70000"),
            ("OMYPIC_RAG_ALLOW_HASH_FALLBACK", "maybe"),
            ("OMYPIC_OPENAI_CONVERSATION_REBASE_TURNS", "-1"),
            ("OMYPIC_ANTHROPIC_MAX_TOKENS", "1.5"),
        ] {
            let error = from_map(&[(name, value)]).unwrap_err();

            match error {
                AppError::InvalidArgument(message) => {
                    assert!(message.contains(name), "{message}");
                    assert!(message.contains(value), "{message}");
                }
                other => panic!("expected InvalidArgument, got {other:?}"),
            }
        }
    }
}
