use chrono::{DateTime, Utc};

use super::{RulebookId, RulebookScope};
use crate::error::{AppError, AppResult};
use crate::question::QuestionGroup;

#[derive(Debug, Clone)]
pub struct Rulebook {
    id: RulebookId,
    filename: String,
    path: String,
    scope: RulebookScope,
    question_group: Option<QuestionGroup>,
    enabled: bool,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl Rulebook {
    pub fn create(
        filename: &str,
        path: &str,
        scope: RulebookScope,
        question_group: Option<QuestionGroup>,
        now: DateTime<Utc>,
    ) -> AppResult<Self> {
        Self::rehydrate(
            RulebookId::new_id(),
            filename,
            path,
            Some(scope),
            question_group,
            true,
            Some(now),
            Some(now),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn rehydrate(
        id: RulebookId,
        filename: &str,
        path: &str,
        scope: Option<RulebookScope>,
        question_group: Option<QuestionGroup>,
        enabled: bool,
        created_at: Option<DateTime<Utc>>,
        updated_at: Option<DateTime<Utc>>,
    ) -> AppResult<Self> {
        let scope = scope.unwrap_or(RulebookScope::Main);
        validate_scope(scope, question_group.as_ref())?;
        let created_at = created_at.unwrap_or_else(Utc::now);
        Ok(Self {
            id,
            filename: filename.to_string(),
            path: path.to_string(),
            scope,
            question_group,
            enabled,
            created_at,
            updated_at: updated_at.unwrap_or(created_at),
        })
    }

    pub fn toggle(&mut self, enabled: bool, now: DateTime<Utc>) {
        self.enabled = enabled;
        self.updated_at = now;
    }

    pub fn id(&self) -> &RulebookId {
        &self.id
    }

    pub fn filename(&self) -> &str {
        &self.filename
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn scope(&self) -> RulebookScope {
        self.scope
    }

    pub fn question_group(&self) -> Option<&QuestionGroup> {
        self.question_group.as_ref()
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

fn validate_scope(scope: RulebookScope, question_group: Option<&QuestionGroup>) -> AppResult<()> {
    if scope == RulebookScope::Main && question_group.is_some() {
        return Err(AppError::InvalidArgument(
            "MAIN scope does not allow questionGroup".to_string(),
        ));
    }
    if scope == RulebookScope::Question && question_group.is_none() {
        return Err(AppError::InvalidArgument(
            "QUESTION scope requires questionGroup".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::AppError;
    use crate::question::QuestionGroup;
    use chrono::{DateTime, Utc};

    #[test]
    fn create_rejects_question_group_for_main_scope() {
        let result = Rulebook::create(
            "main.md",
            "/tmp/main.md",
            RulebookScope::Main,
            Some(QuestionGroup::of("A").unwrap()),
            Utc::now(),
        );

        assert!(matches!(result, Err(AppError::InvalidArgument(_))));
    }

    #[test]
    fn create_requires_question_group_for_question_scope() {
        let result = Rulebook::create(
            "q.md",
            "/tmp/q.md",
            RulebookScope::Question,
            None,
            Utc::now(),
        );

        assert!(matches!(result, Err(AppError::InvalidArgument(_))));
    }

    #[test]
    fn create_accepts_valid_scope_and_group_combination() {
        let rulebook = Rulebook::create(
            "q.md",
            "/tmp/q.md",
            RulebookScope::Question,
            Some(QuestionGroup::of("A").unwrap()),
            "2026-01-01T00:00:00Z".parse::<DateTime<Utc>>().unwrap(),
        )
        .unwrap();

        assert_eq!(RulebookScope::Question, rulebook.scope());
        assert_eq!(
            Some(&QuestionGroup::of("A").unwrap()),
            rulebook.question_group()
        );
    }
}
