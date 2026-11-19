//! 제공자 어댑터가 함께 쓰는 작은 도우미.

use omypic_domain::shared::text_utils::java_is_blank;
use serde_json::Value;

/// 첫 JSON 값만 읽고 뒤에 남은 글자는 무시한다(Jackson `readTree`/`readValue`와 같다).
/// 비어 있는 입력은 Jackson이 "No content"로 거부하므로 오류다.
pub(crate) fn parse_first_json_value(text: &str) -> Result<Value, String> {
    match serde_json::Deserializer::from_str(text)
        .into_iter::<Value>()
        .next()
    {
        Some(Ok(value)) => Ok(value),
        Some(Err(error)) => Err(error.to_string()),
        None => Err("No content to map due to end-of-input".to_string()),
    }
}

/// 비어 있거나 공백뿐인 메시지는 `unknown`으로 바꾼다(로그와 오류 문장용).
pub(crate) fn default_message(message: &str) -> String {
    if java_is_blank(message) {
        "unknown".to_string()
    } else {
        message.to_string()
    }
}
