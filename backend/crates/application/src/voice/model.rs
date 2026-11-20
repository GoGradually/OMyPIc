/// 이벤트를 클라이언트로 보내는 쪽(SSE 등). 보낼 수 없으면(연결이 닫힘) `false`.
/// `send` 안에서 음성 세션 유스케이스를 다시 부르면 안 된다(이벤트 잠금을 쥔 채로 불린다).
pub trait VoiceEventSink: Send + Sync {
    fn send(&self, event: &str, payload: serde_json::Value) -> bool;

    /// 받는 쪽이 이미 떠났으면 `true`. 이벤트를 보내 보지 않고도 죽은 싱크를 뺄 때 쓴다.
    fn is_closed(&self) -> bool {
        false
    }
}

/// 학습자가 오디오 없이 현재 질문에 답을 마쳤다고 알린다. AI가 꺼져 있으면 턴 경계만 오고,
/// 기기 안 음성 인식이면 클라이언트가 만든 전사문도 같이 온다.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VoiceAnswerCommand {
    pub voice_session_id: Option<String>,
    pub sequence: Option<i64>,
    pub text: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VoiceAudioChunkCommand {
    pub voice_session_id: Option<String>,
    pub pcm16_base64: Option<String>,
    pub sample_rate: Option<i32>,
    pub sequence: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VoiceSessionOpenCommand {
    pub session_id: Option<String>,
    pub api_key: Option<String>,
    /// 피드백 LLM 제공자의 키. 음성(STT/TTS) 키와 다를 때만 필요하다(Claude 모델을 고른 경우).
    pub feedback_api_key: Option<String>,
    pub feedback_provider: Option<String>,
    /// `false`면 STT, TTS, 피드백, API 키 없이 돈다. `None`은 켜진 것으로 본다.
    pub ai_enabled: Option<bool>,
    pub feedback_model: Option<String>,
    pub feedback_language: Option<String>,
    pub stt_model: Option<String>,
    pub tts_model: Option<String>,
    pub tts_voice: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceSessionStopCommand {
    pub voice_session_id: Option<String>,
    pub forced: bool,
    pub reason: Option<String>,
}

impl Default for VoiceSessionStopCommand {
    fn default() -> Self {
        Self {
            voice_session_id: None,
            forced: true,
            reason: None,
        }
    }
}
