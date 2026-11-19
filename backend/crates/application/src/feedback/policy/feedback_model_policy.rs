use omypic_domain::error::{AppError, AppResult};
use omypic_domain::shared::text_utils::{java_is_blank, java_trim};

pub const OPENAI_PROVIDER: &str = "openai";
pub const ANTHROPIC_PROVIDER: &str = "anthropic";

const OPENAI_MODELS: [&str; 7] = [
    "gpt-5-mini",
    "gpt-5-nano",
    "gpt-4.1",
    "gpt-4.1-mini",
    "gpt-4.1-nano",
    "gpt-4o",
    "gpt-4o-mini",
];
const ANTHROPIC_MODELS: [&str; 4] = [
    "claude-opus-5-5",
    "claude-sonnet-5-5",
    "claude-haiku-5-5",
    "claude-fable-5-2",
];
const PROVIDERS: [&str; 2] = [OPENAI_PROVIDER, ANTHROPIC_PROVIDER];
const SUPPORTED_MODELS: [&str; 11] = [
    "gpt-5-mini",
    "gpt-5-nano",
    "gpt-4.1",
    "gpt-4.1-mini",
    "gpt-4.1-nano",
    "gpt-4o",
    "gpt-4o-mini",
    "claude-opus-5-5",
    "claude-sonnet-5-5",
    "claude-haiku-5-5",
    "claude-fable-5-2",
];

/// 피드백에 쓸 수 있는 제공자와 모델 목록, 그리고 검증.
pub struct FeedbackModelPolicy;

impl FeedbackModelPolicy {
    pub const OPENAI_PROVIDER: &'static str = OPENAI_PROVIDER;
    pub const ANTHROPIC_PROVIDER: &'static str = ANTHROPIC_PROVIDER;

    /// 모델이 비어 있으면 통과(서버 기본값을 쓴다). 지원하지 않는 모델이면 오류.
    pub fn validate_or_throw(model: Option<&str>) -> AppResult<()> {
        let Some(model) = model else {
            return Ok(());
        };
        if java_is_blank(model) {
            return Ok(());
        }
        if !SUPPORTED_MODELS.contains(&normalize(model).as_str()) {
            return Err(AppError::InvalidArgument(format!(
                "Unsupported feedback model: {}",
                java_trim(model)
            )));
        }
        Ok(())
    }

    /// 모델 검증에 더해 모델이 주어진 제공자의 것인지도 확인한다.
    pub fn validate_or_throw_for_provider(
        provider: Option<&str>,
        model: Option<&str>,
    ) -> AppResult<()> {
        Self::validate_or_throw(model)?;
        let (Some(provider), Some(model)) = (provider, model) else {
            return Ok(());
        };
        if java_is_blank(model) || java_is_blank(provider) {
            return Ok(());
        }
        let expected = Self::normalize_provider(Some(provider))?;
        if expected != Self::provider_of(Some(model)) {
            return Err(AppError::InvalidArgument(format!(
                "Feedback model {} does not belong to provider {}",
                java_trim(model),
                expected
            )));
        }
        Ok(())
    }

    pub fn supported_models() -> &'static [&'static str] {
        &SUPPORTED_MODELS
    }

    pub fn supported_models_for(provider: Option<&str>) -> AppResult<&'static [&'static str]> {
        let provider = Self::normalize_provider(provider)?;
        Ok(if provider == ANTHROPIC_PROVIDER {
            &ANTHROPIC_MODELS
        } else {
            &OPENAI_MODELS
        })
    }

    pub fn supported_providers() -> &'static [&'static str] {
        &PROVIDERS
    }

    pub fn default_model(provider: Option<&str>) -> AppResult<&'static str> {
        Ok(Self::supported_models_for(provider)?[0])
    }

    /// 모델을 서비스하는 제공자. 모델이 비어 있으면 OpenAI다(서버 기본값 동작을 그대로 둔다).
    pub fn provider_of(model: Option<&str>) -> &'static str {
        match model {
            Some(model)
                if !java_is_blank(model)
                    && ANTHROPIC_MODELS.contains(&normalize(model).as_str()) =>
            {
                ANTHROPIC_PROVIDER
            }
            _ => OPENAI_PROVIDER,
        }
    }

    pub fn normalize_provider(provider: Option<&str>) -> AppResult<&'static str> {
        let provider = match provider {
            Some(provider) if !java_is_blank(provider) => provider,
            _ => {
                return Err(AppError::InvalidArgument(
                    "Provider is required".to_string(),
                ))
            }
        };
        let normalized = java_trim(provider).to_lowercase();
        PROVIDERS
            .iter()
            .find(|p| **p == normalized)
            .copied()
            .ok_or_else(|| {
                AppError::InvalidArgument(
                    "Unsupported provider: only openai and anthropic are allowed".to_string(),
                )
            })
    }
}

fn normalize(model: &str) -> String {
    java_trim(model).to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_or_throw_allows_supported_model() {
        assert!(FeedbackModelPolicy::validate_or_throw(Some("gpt-5-mini")).is_ok());
    }

    #[test]
    fn validate_or_throw_allows_blank_model_for_fallback() {
        assert!(FeedbackModelPolicy::validate_or_throw(Some(" ")).is_ok());
    }

    #[test]
    fn validate_or_throw_rejects_unsupported_model() {
        let error = FeedbackModelPolicy::validate_or_throw(Some("gpt-5.2")).unwrap_err();

        assert_eq!(
            AppError::InvalidArgument("Unsupported feedback model: gpt-5.2".to_string()),
            error
        );
    }

    #[test]
    fn validate_or_throw_allows_claude_models() {
        assert!(FeedbackModelPolicy::validate_or_throw(Some("claude-opus-5-5")).is_ok());
        assert!(FeedbackModelPolicy::validate_or_throw_for_provider(
            Some("anthropic"),
            Some("claude-haiku-5-5")
        )
        .is_ok());
    }

    #[test]
    fn validate_or_throw_rejects_model_from_another_provider() {
        let error = FeedbackModelPolicy::validate_or_throw_for_provider(
            Some("anthropic"),
            Some("gpt-5-mini"),
        )
        .unwrap_err();

        assert_eq!(
            AppError::InvalidArgument(
                "Feedback model gpt-5-mini does not belong to provider anthropic".to_string()
            ),
            error
        );
    }

    #[test]
    fn provider_of_resolves_provider_from_model_id() {
        assert_eq!(
            "anthropic",
            FeedbackModelPolicy::provider_of(Some("claude-sonnet-5-5"))
        );
        assert_eq!(
            "openai",
            FeedbackModelPolicy::provider_of(Some("gpt-5-nano"))
        );
        assert_eq!("openai", FeedbackModelPolicy::provider_of(Some(" ")));
    }

    #[test]
    fn supported_models_groups_models_by_provider() {
        assert_eq!(
            "claude-opus-5-5",
            FeedbackModelPolicy::default_model(Some("anthropic")).unwrap()
        );
        assert_eq!(
            4,
            FeedbackModelPolicy::supported_models_for(Some("anthropic"))
                .unwrap()
                .len()
        );
        assert_eq!(
            7,
            FeedbackModelPolicy::supported_models_for(Some("openai"))
                .unwrap()
                .len()
        );
        assert_eq!(11, FeedbackModelPolicy::supported_models().len());
    }

    #[test]
    fn supported_models_is_openai_followed_by_anthropic() {
        let expected: Vec<&str> = OPENAI_MODELS
            .iter()
            .chain(&ANTHROPIC_MODELS)
            .copied()
            .collect();

        assert_eq!(expected, SUPPORTED_MODELS.to_vec());
    }

    #[test]
    fn normalize_provider_rejects_unknown_provider() {
        assert!(matches!(
            FeedbackModelPolicy::normalize_provider(Some("gemini")),
            Err(AppError::InvalidArgument(_))
        ));
    }
}
