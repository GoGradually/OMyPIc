use std::sync::Mutex;

use omypic_domain::error::{AppError, AppResult};
use omypic_domain::session::{LlmConversationState, LlmPromptContext};

use super::lock;
use crate::apikey::port::ApiKeyProbePort;
use crate::feedback::port::{LlmClient, LlmGenerateResult};
use crate::rulebook::port::EmbeddingPort;
use crate::stt::model::VadSettings;
use crate::stt::port::SttGateway;
use crate::voice::port::TtsGateway;

/// `probe` 호출 하나.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeCall {
    pub provider: String,
    pub api_key: String,
    pub model: Option<String>,
}

/// 기본은 성공한다. `fail_with`로 이후 호출을 실패시킨다.
#[derive(Debug, Default)]
pub struct FakeApiKeyProbePort {
    error: Mutex<Option<AppError>>,
    calls: Mutex<Vec<ProbeCall>>,
}

impl FakeApiKeyProbePort {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn fail_with(&self, error: AppError) {
        *lock(&self.error) = Some(error);
    }

    pub fn calls(&self) -> Vec<ProbeCall> {
        lock(&self.calls).clone()
    }
}

impl ApiKeyProbePort for FakeApiKeyProbePort {
    fn probe(&self, provider: &str, api_key: &str, model: Option<&str>) -> AppResult<()> {
        lock(&self.calls).push(ProbeCall {
            provider: provider.to_string(),
            api_key: api_key.to_string(),
            model: model.map(str::to_string),
        });
        match lock(&self.error).clone() {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

/// `transcribe` 호출 하나.
#[derive(Debug, Clone, PartialEq)]
pub struct SttCall {
    pub file_bytes: Vec<u8>,
    pub model: String,
    pub api_key: String,
    pub translate: bool,
    pub vad_settings: VadSettings,
}

/// 호출마다 대본의 다음 결과를 돌려준다. 대본이 끝나면 마지막 결과를 계속 돌려준다
/// (Mockito의 `thenThrow(...).thenReturn(...)`와 같다).
#[derive(Debug)]
pub struct ScriptedSttGateway {
    script: Mutex<Vec<AppResult<String>>>,
    calls: Mutex<Vec<SttCall>>,
}

impl ScriptedSttGateway {
    pub fn new(script: Vec<AppResult<String>>) -> Self {
        assert!(!script.is_empty(), "script must have at least one result");
        Self {
            script: Mutex::new(script),
            calls: Mutex::new(Vec::new()),
        }
    }

    pub fn calls(&self) -> Vec<SttCall> {
        lock(&self.calls).clone()
    }
}

impl SttGateway for ScriptedSttGateway {
    fn transcribe(
        &self,
        file_bytes: &[u8],
        model: &str,
        api_key: &str,
        translate: bool,
        vad_settings: &VadSettings,
    ) -> AppResult<String> {
        lock(&self.calls).push(SttCall {
            file_bytes: file_bytes.to_vec(),
            model: model.to_string(),
            api_key: api_key.to_string(),
            translate,
            vad_settings: *vad_settings,
        });
        next_scripted(&self.script)
    }
}

fn next_scripted<T: Clone>(script: &Mutex<Vec<AppResult<T>>>) -> AppResult<T> {
    let mut script = lock(script);
    if script.len() > 1 {
        script.remove(0)
    } else {
        script[0].clone()
    }
}

/// `synthesize` 호출 하나.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TtsCall {
    pub api_key: String,
    pub model: String,
    pub voice: String,
    pub text: String,
}

#[derive(Debug)]
pub struct FakeTtsGateway {
    audio: Vec<u8>,
    error: Mutex<Option<AppError>>,
    calls: Mutex<Vec<TtsCall>>,
}

impl FakeTtsGateway {
    pub fn new(audio: &[u8]) -> Self {
        Self {
            audio: audio.to_vec(),
            error: Mutex::new(None),
            calls: Mutex::new(Vec::new()),
        }
    }

    pub fn fail_with(&self, error: AppError) {
        *lock(&self.error) = Some(error);
    }

    pub fn calls(&self) -> Vec<TtsCall> {
        lock(&self.calls).clone()
    }
}

impl TtsGateway for FakeTtsGateway {
    fn synthesize(
        &self,
        api_key: &str,
        model: &str,
        voice: &str,
        text: &str,
    ) -> AppResult<Vec<u8>> {
        lock(&self.calls).push(TtsCall {
            api_key: api_key.to_string(),
            model: model.to_string(),
            voice: voice.to_string(),
            text: text.to_string(),
        });
        match lock(&self.error).clone() {
            Some(error) => Err(error),
            None => Ok(self.audio.clone()),
        }
    }
}

/// 같은 입력이면 같은 벡터를 돌려준다. 첫 칸이 글자 수이고 나머지는 0이다.
#[derive(Debug)]
pub struct FakeEmbeddingPort {
    dimension: usize,
    texts: Mutex<Vec<String>>,
}

impl FakeEmbeddingPort {
    pub fn new(dimension: usize) -> Self {
        Self {
            dimension,
            texts: Mutex::new(Vec::new()),
        }
    }

    pub fn embedded_texts(&self) -> Vec<String> {
        lock(&self.texts).clone()
    }
}

impl EmbeddingPort for FakeEmbeddingPort {
    fn embed(&self, text: &str) -> AppResult<Vec<f32>> {
        lock(&self.texts).push(text.to_string());
        let mut vector = vec![0.0; self.dimension];
        if let Some(first) = vector.first_mut() {
            *first = text.chars().count() as f32;
        }
        Ok(vector)
    }

    fn dimension(&self) -> usize {
        self.dimension
    }
}

/// `generate` 호출 하나.
#[derive(Debug, Clone)]
pub struct GenerateCall {
    pub api_key: String,
    pub model: String,
    pub system_prompt: String,
    pub user_prompt: String,
    pub conversation_state: LlmConversationState,
    pub prompt_context: LlmPromptContext,
}

/// `bootstrap` 호출 하나.
#[derive(Debug, Clone)]
pub struct BootstrapCall {
    pub api_key: String,
    pub model: String,
    pub system_prompt: String,
    pub conversation_state: LlmConversationState,
}

/// 대본의 결과를 차례로 돌려주는 LLM 클라이언트. `bootstrap`은 정해 둔 상태를 돌려준다.
#[derive(Debug)]
pub struct ScriptedLlmClient {
    provider: String,
    stateless: bool,
    bootstrap_state: Mutex<AppResult<LlmConversationState>>,
    script: Mutex<Vec<AppResult<LlmGenerateResult>>>,
    bootstrap_calls: Mutex<Vec<BootstrapCall>>,
    generate_calls: Mutex<Vec<GenerateCall>>,
}

impl ScriptedLlmClient {
    pub fn new(provider: &str, stateless: bool, script: Vec<AppResult<LlmGenerateResult>>) -> Self {
        assert!(!script.is_empty(), "script must have at least one result");
        Self {
            provider: provider.to_string(),
            stateless,
            bootstrap_state: Mutex::new(Ok(LlmConversationState::empty())),
            script: Mutex::new(script),
            bootstrap_calls: Mutex::new(Vec::new()),
            generate_calls: Mutex::new(Vec::new()),
        }
    }

    pub fn set_bootstrap_result(&self, result: AppResult<LlmConversationState>) {
        *lock(&self.bootstrap_state) = result;
    }

    pub fn bootstrap_calls(&self) -> Vec<BootstrapCall> {
        lock(&self.bootstrap_calls).clone()
    }

    pub fn generate_calls(&self) -> Vec<GenerateCall> {
        lock(&self.generate_calls).clone()
    }
}

impl LlmClient for ScriptedLlmClient {
    fn provider(&self) -> String {
        self.provider.clone()
    }

    fn is_stateless(&self) -> bool {
        self.stateless
    }

    fn bootstrap(
        &self,
        api_key: &str,
        model: &str,
        system_prompt: &str,
        conversation_state: &LlmConversationState,
    ) -> AppResult<LlmConversationState> {
        lock(&self.bootstrap_calls).push(BootstrapCall {
            api_key: api_key.to_string(),
            model: model.to_string(),
            system_prompt: system_prompt.to_string(),
            conversation_state: conversation_state.clone(),
        });
        lock(&self.bootstrap_state).clone()
    }

    fn generate(
        &self,
        api_key: &str,
        model: &str,
        system_prompt: &str,
        user_prompt: &str,
        conversation_state: &LlmConversationState,
        prompt_context: &LlmPromptContext,
    ) -> AppResult<LlmGenerateResult> {
        lock(&self.generate_calls).push(GenerateCall {
            api_key: api_key.to_string(),
            model: model.to_string(),
            system_prompt: system_prompt.to_string(),
            user_prompt: user_prompt.to_string(),
            conversation_state: conversation_state.clone(),
            prompt_context: prompt_context.clone(),
        });
        next_scripted(&self.script)
    }
}
