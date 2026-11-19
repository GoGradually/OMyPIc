use std::collections::BTreeSet;

use omypic_domain::shared::text_utils::{java_is_blank, utf16_len, utf16_prefix};
use serde_json::{Map, Value};

use crate::config::OpenAiLoggingConfig;

const DEFAULT_RESPONSE_PREVIEW_CHARS: usize = 1024;
const DEFAULT_FULL_BODY: bool = false;
const DEFAULT_LOG_SUCCESS_AT_FINE: bool = true;

/// 로그에 남길 응답 본문의 길이와 형식을 정한다. API 키와 `Authorization` 헤더는
/// 이 값들을 받지 않으므로 로그에 들어갈 길이 없다.
#[derive(Debug, Clone)]
pub struct OpenAiLlmLogFormatter {
    response_preview_chars: usize,
    full_body: bool,
    log_success_at_fine: bool,
}

impl OpenAiLlmLogFormatter {
    pub fn new(response_preview_chars: usize, full_body: bool, log_success_at_fine: bool) -> Self {
        Self {
            response_preview_chars,
            full_body,
            log_success_at_fine,
        }
    }

    pub fn from_config(logging: &OpenAiLoggingConfig) -> Self {
        Self::new(
            logging.response_preview_chars,
            logging.full_body,
            logging.log_success_at_fine,
        )
    }

    pub fn defaults() -> Self {
        Self::new(
            DEFAULT_RESPONSE_PREVIEW_CHARS,
            DEFAULT_FULL_BODY,
            DEFAULT_LOG_SUCCESS_AT_FINE,
        )
    }

    pub fn should_log_success_at_fine(&self) -> bool {
        self.log_success_at_fine
    }

    pub fn response_preview(&self, body: &str) -> String {
        if java_is_blank(body) {
            return String::new();
        }
        let normalized = normalize_for_log(body);
        if self.full_body {
            return normalized;
        }
        if self.response_preview_chars == 0 || utf16_len(&normalized) <= self.response_preview_chars
        {
            return normalized;
        }
        format!(
            "{}...(truncated)",
            utf16_prefix(&normalized, self.response_preview_chars)
        )
    }

    /// 요청 본문에 실제로 들어간 선택 파라미터 이름을 정렬해 `[a, b]` 꼴로 돌려준다.
    pub fn optional_parameter_summary(
        &self,
        payload: &Map<String, Value>,
        optional_parameters: &BTreeSet<String>,
    ) -> String {
        let included: Vec<&str> = optional_parameters
            .iter()
            .filter(|key| payload.contains_key(key.as_str()))
            .map(String::as_str)
            .collect();
        format!("[{}]", included.join(", "))
    }
}

fn normalize_for_log(text: &str) -> String {
    text.replace('\r', "\\r").replace('\n', "\\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn response_preview_truncates_and_escapes_line_breaks() {
        let formatter = OpenAiLlmLogFormatter::new(8, false, true);

        assert_eq!("a\\nb\\rc", formatter.response_preview("a\nb\rc"));
        assert_eq!(
            "12345678...(truncated)",
            formatter.response_preview("123456789")
        );
        assert_eq!("12345678", formatter.response_preview("12345678"));
        assert_eq!("", formatter.response_preview("  \n"));
    }

    #[test]
    fn response_preview_keeps_everything_for_full_body_or_zero_limit() {
        let long = "x".repeat(3000);

        assert_eq!(
            long,
            OpenAiLlmLogFormatter::new(8, true, true).response_preview(&long)
        );
        assert_eq!(
            long,
            OpenAiLlmLogFormatter::new(0, false, true).response_preview(&long)
        );
    }

    #[test]
    fn defaults_match_java_defaults() {
        let formatter = OpenAiLlmLogFormatter::defaults();
        let long = "y".repeat(2000);

        assert!(formatter.should_log_success_at_fine());
        assert_eq!(
            format!("{}...(truncated)", "y".repeat(1024)),
            formatter.response_preview(&long)
        );
    }

    #[test]
    fn optional_parameter_summary_lists_present_keys_sorted() {
        let formatter = OpenAiLlmLogFormatter::defaults();
        let payload = json!({"temperature": 0.2, "top_p": 1})
            .as_object()
            .unwrap()
            .clone();
        let keys: BTreeSet<String> = ["top_p", "temperature", "seed"]
            .iter()
            .map(|key| key.to_string())
            .collect();

        assert_eq!(
            "[temperature, top_p]",
            formatter.optional_parameter_summary(&payload, &keys)
        );
        assert_eq!(
            "[]",
            formatter.optional_parameter_summary(&Map::new(), &keys)
        );
    }
}
