use std::sync::Arc;

use omypic_domain::error::AppResult;
use omypic_domain::shared::text_utils::java_is_blank;

use super::model::{ApiKeyVerifyCommand, ApiKeyVerifyResult};
use super::port::ApiKeyProbePort;
use crate::feedback::policy::FeedbackModelPolicy;

const OPENAI_KEY_PREFIX: &str = "sk-";
const ANTHROPIC_KEY_PREFIX: &str = "sk-ant-";

pub struct ApiKeyVerifyUseCase {
    probe_port: Arc<dyn ApiKeyProbePort>,
}

impl ApiKeyVerifyUseCase {
    pub fn new(probe_port: Arc<dyn ApiKeyProbePort>) -> Self {
        Self { probe_port }
    }

    /// 키 형식이나 제공자 응답이 잘못된 것은 실패 결과로 돌려주고, 제공자나 모델이
    /// 지원 대상이 아닌 것은 오류로 돌려준다.
    pub fn verify(&self, command: &ApiKeyVerifyCommand) -> AppResult<ApiKeyVerifyResult> {
        let provider = FeedbackModelPolicy::normalize_provider(command.provider.as_deref())?;
        let api_key = command.api_key.as_deref();
        if let Some(format_error) = validate_format(provider, api_key) {
            return Ok(ApiKeyVerifyResult::failure(provider, &format_error));
        }
        FeedbackModelPolicy::validate_or_throw_for_provider(
            Some(provider),
            command.model.as_deref(),
        )?;
        Ok(self.verify_provider(provider, api_key.unwrap_or(""), command.model.as_deref()))
    }

    fn verify_provider(
        &self,
        provider: &str,
        api_key: &str,
        model: Option<&str>,
    ) -> ApiKeyVerifyResult {
        match self.probe_port.probe(provider, api_key, model) {
            Ok(()) => ApiKeyVerifyResult::success(provider),
            Err(error) => {
                ApiKeyVerifyResult::failure(provider, &failure_message(&error.to_string()))
            }
        }
    }
}

fn failure_message(message: &str) -> String {
    if java_is_blank(message) {
        "Provider verification failed".to_string()
    } else {
        message.to_string()
    }
}

fn validate_format(provider: &str, api_key: Option<&str>) -> Option<String> {
    let api_key = match api_key {
        Some(key) if !java_is_blank(key) => key,
        _ => return Some("API key is required".to_string()),
    };
    if provider == FeedbackModelPolicy::ANTHROPIC_PROVIDER {
        return (!api_key.starts_with(ANTHROPIC_KEY_PREFIX))
            .then(|| "Anthropic key must start with sk-ant-".to_string());
    }
    if api_key.starts_with(ANTHROPIC_KEY_PREFIX) {
        return Some(
            "This looks like an Anthropic key. Save it as the Claude API key instead.".to_string(),
        );
    }
    (!api_key.starts_with(OPENAI_KEY_PREFIX)).then(|| "OpenAI key must start with sk-".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{FakeApiKeyProbePort, ProbeCall};
    use omypic_domain::error::AppError;

    fn setup() -> (Arc<FakeApiKeyProbePort>, ApiKeyVerifyUseCase) {
        let probe = Arc::new(FakeApiKeyProbePort::new());
        let use_case = ApiKeyVerifyUseCase::new(probe.clone());
        (probe, use_case)
    }

    fn command(provider: &str, key: &str, model: Option<&str>) -> ApiKeyVerifyCommand {
        ApiKeyVerifyCommand {
            provider: Some(provider.to_string()),
            api_key: Some(key.to_string()),
            model: model.map(str::to_string),
        }
    }

    #[test]
    fn verify_returns_failure_when_format_invalid() {
        let (probe, use_case) = setup();

        let result = use_case
            .verify(&command("openai", "invalid", None))
            .unwrap();

        assert!(!result.valid);
        assert!(result.message.contains("sk-"));
        assert!(probe.calls().is_empty());
    }

    #[test]
    fn verify_returns_success_when_probe_passes() {
        let (probe, use_case) = setup();

        let result = use_case
            .verify(&command("openai", "sk-test-key", None))
            .unwrap();

        assert!(result.valid);
        assert_eq!(
            vec![ProbeCall {
                provider: "openai".to_string(),
                api_key: "sk-test-key".to_string(),
                model: None
            }],
            probe.calls()
        );
    }

    #[test]
    fn verify_returns_failure_when_probe_fails() {
        let (probe, use_case) = setup();
        probe.fail_with(AppError::InvalidState("401".to_string()));

        let result = use_case
            .verify(&command("openai", "sk-test", None))
            .unwrap();

        assert!(!result.valid);
        assert_eq!("401", result.message);
    }

    #[test]
    fn verify_returns_default_message_when_probe_error_message_is_blank() {
        let (probe, use_case) = setup();
        probe.fail_with(AppError::Internal("  ".to_string()));

        let result = use_case
            .verify(&command("openai", "sk-test", None))
            .unwrap();

        assert_eq!("Provider verification failed", result.message);
    }

    #[test]
    fn verify_throws_when_feedback_model_is_unsupported() {
        let (probe, use_case) = setup();

        let error = use_case
            .verify(&command("openai", "sk-test-key", Some("gpt-5.2")))
            .unwrap_err();

        assert_eq!(
            AppError::InvalidArgument("Unsupported feedback model: gpt-5.2".to_string()),
            error
        );
        assert!(probe.calls().is_empty());
    }

    #[test]
    fn verify_throws_when_provider_is_unsupported() {
        let (probe, use_case) = setup();

        let error = use_case
            .verify(&command("gemini", "AIza-test-key", None))
            .unwrap_err();

        assert_eq!(
            AppError::InvalidArgument(
                "Unsupported provider: only openai and anthropic are allowed".to_string()
            ),
            error
        );
        assert!(probe.calls().is_empty());
    }

    #[test]
    fn verify_probes_anthropic_when_claude_key_and_model_are_given() {
        let (probe, use_case) = setup();

        let result = use_case
            .verify(&command(
                "Anthropic",
                "sk-ant-test-key",
                Some("claude-opus-5-5"),
            ))
            .unwrap();

        assert!(result.valid);
        assert_eq!("anthropic", result.provider);
        assert_eq!(
            vec![ProbeCall {
                provider: "anthropic".to_string(),
                api_key: "sk-ant-test-key".to_string(),
                model: Some("claude-opus-5-5".to_string())
            }],
            probe.calls()
        );
    }

    #[test]
    fn verify_returns_failure_when_anthropic_key_format_invalid() {
        let (probe, use_case) = setup();

        let result = use_case
            .verify(&command("anthropic", "sk-openai-style", None))
            .unwrap();

        assert!(!result.valid);
        assert!(result.message.contains("sk-ant-"));
        assert!(probe.calls().is_empty());
    }

    #[test]
    fn verify_returns_failure_when_anthropic_key_is_saved_as_open_ai_key() {
        let (probe, use_case) = setup();

        let result = use_case
            .verify(&command("openai", "sk-ant-test-key", None))
            .unwrap();

        assert!(!result.valid);
        assert!(result.message.contains("Anthropic"));
        assert!(probe.calls().is_empty());
    }

    #[test]
    fn verify_throws_when_model_belongs_to_another_provider() {
        let (probe, use_case) = setup();

        let error = use_case
            .verify(&command("anthropic", "sk-ant-test-key", Some("gpt-5-mini")))
            .unwrap_err();

        assert!(matches!(error, AppError::InvalidArgument(_)));
        assert!(probe.calls().is_empty());
    }

    #[test]
    fn verify_returns_failure_when_api_key_is_missing() {
        let (_, use_case) = setup();
        let command = ApiKeyVerifyCommand {
            provider: Some("openai".to_string()),
            ..ApiKeyVerifyCommand::default()
        };

        let result = use_case.verify(&command).unwrap();

        assert!(!result.valid);
        assert_eq!("API key is required", result.message);
    }
}
