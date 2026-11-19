use omypic_domain::error::{AppError, AppResult};
use serde_json::{Map, Value};

/// 모델 계열마다 요청에 넣을 수 있는 선택 파라미터가 달라서 정책으로 나눈다.
pub trait OpenAiModelParameterPolicy: Send + Sync {
    fn supports(&self, model: &str) -> bool;
    fn apply(&self, payload: &mut Map<String, Value>);
}

/// GPT-5 계열은 이 클라이언트 경로에서 temperature를 무시한다.
pub struct Gpt5ModelParameterPolicy;

impl OpenAiModelParameterPolicy for Gpt5ModelParameterPolicy {
    fn supports(&self, model: &str) -> bool {
        model.to_lowercase().starts_with("gpt-5")
    }

    fn apply(&self, _payload: &mut Map<String, Value>) {}
}

pub struct LegacyChatModelParameterPolicy;

impl OpenAiModelParameterPolicy for LegacyChatModelParameterPolicy {
    fn supports(&self, _model: &str) -> bool {
        true
    }

    fn apply(&self, payload: &mut Map<String, Value>) {
        payload.insert("temperature".to_string(), Value::from(0.2));
    }
}

pub struct OpenAiModelParameterPolicyResolver {
    policies: Vec<Box<dyn OpenAiModelParameterPolicy>>,
}

impl OpenAiModelParameterPolicyResolver {
    pub fn new() -> Self {
        Self::with_policies(vec![
            Box::new(Gpt5ModelParameterPolicy),
            Box::new(LegacyChatModelParameterPolicy),
        ])
    }

    pub fn with_policies(policies: Vec<Box<dyn OpenAiModelParameterPolicy>>) -> Self {
        Self { policies }
    }

    pub fn resolve(&self, model: &str) -> AppResult<&dyn OpenAiModelParameterPolicy> {
        self.policies
            .iter()
            .find(|policy| policy.supports(model))
            .map(|policy| policy.as_ref())
            .ok_or_else(|| {
                AppError::InvalidState(format!(
                    "No OpenAI model parameter policy for model: {model}"
                ))
            })
    }
}

impl Default for OpenAiModelParameterPolicyResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_returns_gpt5_policy_for_gpt5_family() {
        let resolver = OpenAiModelParameterPolicyResolver::new();
        let policy = resolver.resolve("gpt-5-mini").unwrap();
        let mut payload = Map::new();

        policy.apply(&mut payload);

        assert!(!payload.contains_key("temperature"));
    }

    #[test]
    fn resolve_returns_legacy_policy_for_non_gpt5_model() {
        let resolver = OpenAiModelParameterPolicyResolver::new();
        let policy = resolver.resolve("gpt-4o-mini").unwrap();
        let mut payload = Map::new();

        policy.apply(&mut payload);

        assert!(payload.contains_key("temperature"));
    }

    #[test]
    fn resolve_fails_when_no_policy_supports_the_model() {
        let resolver = OpenAiModelParameterPolicyResolver::with_policies(vec![]);

        let error = resolver.resolve("gpt-4o").err().unwrap();

        assert_eq!(
            AppError::InvalidState("No OpenAI model parameter policy for model: gpt-4o".into()),
            error
        );
    }
}
