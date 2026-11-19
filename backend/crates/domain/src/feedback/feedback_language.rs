use crate::shared::text_utils::java_is_blank;

/// `ko` 또는 `en`. 비었거나 알 수 없는 값은 `ko`가 된다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedbackLanguage {
    value: String,
}

impl FeedbackLanguage {
    pub fn of(value: &str) -> Self {
        let normalized = if java_is_blank(value) {
            "ko".to_string()
        } else {
            value.to_lowercase()
        };
        let value = if normalized == "ko" || normalized == "en" {
            normalized
        } else {
            "ko".to_string()
        };
        Self { value }
    }

    pub fn value(&self) -> &str {
        &self.value
    }

    pub fn is_english(&self) -> bool {
        self.value == "en"
    }
}

impl Default for FeedbackLanguage {
    fn default() -> Self {
        Self::of("ko")
    }
}
