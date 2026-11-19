use crate::shared::text_utils::java_trim;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RulebookScope {
    Main,
    Question,
}

impl RulebookScope {
    /// 비어 있거나 알 수 없는 값은 `Main`.
    #[allow(clippy::should_implement_trait)]
    pub fn from(value: Option<&str>) -> Self {
        match value.map(|v| java_trim(v).to_uppercase()) {
            Some(v) if v == "QUESTION" => Self::Question,
            _ => Self::Main,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_does_not_trim_no_break_space_like_java() {
        assert_eq!(
            RulebookScope::Main,
            RulebookScope::from(Some("\u{00A0}question"))
        );
        assert_eq!(
            RulebookScope::Question,
            RulebookScope::from(Some(" question "))
        );
    }
}
