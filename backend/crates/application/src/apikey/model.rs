use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ApiKeyVerifyCommand {
    pub provider: Option<String>,
    pub api_key: Option<String>,
    pub model: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiKeyVerifyResult {
    pub valid: bool,
    pub provider: String,
    pub checked_at: DateTime<Utc>,
    pub message: String,
}

impl ApiKeyVerifyResult {
    pub fn success(provider: &str) -> Self {
        Self {
            valid: true,
            provider: provider.to_string(),
            checked_at: Utc::now(),
            message: "Verified".to_string(),
        }
    }

    pub fn failure(provider: &str, message: &str) -> Self {
        Self {
            valid: false,
            provider: provider.to_string(),
            checked_at: Utc::now(),
            message: message.to_string(),
        }
    }
}
