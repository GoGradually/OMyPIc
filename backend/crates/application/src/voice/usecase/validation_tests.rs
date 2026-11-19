//! Java `VoiceSessionUseCaseValidationTest`. Java는 정책 mock이 모두 null을 돌려주지만, 검사하는
//! 값은 명령에서 오므로 기본 가짜 정책을 쓴다.

use std::sync::Arc;

use omypic_domain::error::AppError;

use super::tests::Fixture;
use crate::test_support::{FakeVoicePolicy, ImmediateExecutor};
use crate::voice::model::VoiceSessionOpenCommand;

fn fixture() -> Fixture {
    Fixture::with(FakeVoicePolicy::default(), Arc::new(ImmediateExecutor))
}

#[test]
fn open_throws_when_feedback_model_is_unsupported() {
    let use_case = fixture().use_case();
    let command = VoiceSessionOpenCommand {
        session_id: Some("s1".to_string()),
        api_key: Some("api-key".to_string()),
        feedback_model: Some("gpt-5.2".to_string()),
        ..VoiceSessionOpenCommand::default()
    };

    let error = use_case.open(&command).unwrap_err();

    assert_eq!(
        AppError::InvalidArgument("Unsupported feedback model: gpt-5.2".to_string()),
        error
    );
}

#[test]
fn open_requires_api_key_while_ai_is_on() {
    let use_case = fixture().use_case();
    let command = VoiceSessionOpenCommand {
        session_id: Some("s1".to_string()),
        ..VoiceSessionOpenCommand::default()
    };

    let error = use_case.open(&command).unwrap_err();

    assert_eq!(
        AppError::InvalidArgument("sessionId and apiKey are required".to_string()),
        error
    );
}

#[test]
fn open_requires_open_ai_key_when_only_one_speech_side_is_local() {
    let use_case = fixture().use_case();
    let command = VoiceSessionOpenCommand {
        session_id: Some("s1".to_string()),
        feedback_model: Some("claude-opus-5-5".to_string()),
        feedback_api_key: Some("sk-ant-key".to_string()),
        stt_model: Some("local-whisper".to_string()),
        tts_model: Some("gpt-4o-mini-tts".to_string()),
        ..VoiceSessionOpenCommand::default()
    };

    let error = use_case.open(&command).unwrap_err();

    assert_eq!(
        AppError::InvalidArgument("sessionId and apiKey are required".to_string()),
        error
    );
}
