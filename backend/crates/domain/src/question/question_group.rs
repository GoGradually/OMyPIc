use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct QuestionGroup {
    value: String,
}

impl QuestionGroup {
    pub fn of(raw: &str) -> AppResult<Self> {
        let normalized = raw.trim();
        if normalized.is_empty() {
            return Err(AppError::InvalidArgument(
                "questionGroup must not be blank".to_string(),
            ));
        }
        Ok(Self {
            value: normalized.to_string(),
        })
    }

    pub fn from_nullable(raw: Option<&str>) -> Option<Self> {
        let raw = raw?;
        if raw.trim().is_empty() {
            return None;
        }
        Self::of(raw).ok()
    }

    pub fn value(&self) -> &str {
        &self.value
    }
}

impl std::fmt::Display for QuestionGroup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    fn hash_of(group: &QuestionGroup) -> u64 {
        let mut hasher = DefaultHasher::new();
        group.hash(&mut hasher);
        hasher.finish()
    }

    #[test]
    fn of_trims_value() {
        let group = QuestionGroup::of("  Group A  ").unwrap();

        assert_eq!("Group A", group.value());
    }

    #[test]
    fn of_throws_when_blank() {
        assert!(matches!(
            QuestionGroup::of("  "),
            Err(crate::error::AppError::InvalidArgument(_))
        ));
    }

    #[test]
    fn from_nullable_returns_null_for_blank() {
        assert!(QuestionGroup::from_nullable(None).is_none());
        assert!(QuestionGroup::from_nullable(Some(" ")).is_none());
    }

    #[test]
    fn equals_and_hash_code_use_value() {
        let left = QuestionGroup::of("A").unwrap();
        let right = QuestionGroup::of("A").unwrap();

        assert_eq!(left, right);
        assert_eq!(hash_of(&left), hash_of(&right));
    }
}
