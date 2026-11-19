#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RulebookScope {
    Main,
    Question,
}

impl RulebookScope {
    /// 비어 있거나 알 수 없는 값은 `Main`.
    #[allow(clippy::should_implement_trait)]
    pub fn from(value: Option<&str>) -> Self {
        match value.map(|v| v.trim().to_uppercase()) {
            Some(v) if v == "QUESTION" => Self::Question,
            _ => Self::Main,
        }
    }
}
