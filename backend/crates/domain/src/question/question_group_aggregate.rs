use chrono::{DateTime, Utc};

use super::{QuestionGroupId, QuestionItem, QuestionItemId};
use crate::error::{AppError, AppResult};
use crate::shared::text_utils::{java_is_blank, java_trim};

const MAX_QUESTIONS_PER_GROUP: usize = 3;
const MAX_QUESTIONS_MESSAGE: &str = "Question group can contain up to 3 questions";

#[derive(Debug, Clone)]
pub struct QuestionGroupAggregate {
    id: QuestionGroupId,
    name: String,
    /// 정규화된 태그. 입력 순서를 유지하고 중복이 없다.
    tags: Vec<String>,
    questions: Vec<QuestionItem>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl QuestionGroupAggregate {
    pub fn create<S: AsRef<str>>(
        name: &str,
        tags: impl IntoIterator<Item = S>,
        now: DateTime<Utc>,
    ) -> Self {
        Self {
            id: QuestionGroupId::new_id(),
            name: java_trim(name).to_string(),
            tags: normalize_tags(tags),
            questions: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn rehydrate<S: AsRef<str>>(
        id: QuestionGroupId,
        name: &str,
        tags: impl IntoIterator<Item = S>,
        questions: Vec<QuestionItem>,
        created_at: Option<DateTime<Utc>>,
        updated_at: Option<DateTime<Utc>>,
    ) -> AppResult<Self> {
        if questions.len() > MAX_QUESTIONS_PER_GROUP {
            return Err(AppError::InvalidArgument(MAX_QUESTIONS_MESSAGE.to_string()));
        }
        let created_at = created_at.unwrap_or_else(Utc::now);
        Ok(Self {
            id,
            name: java_trim(name).to_string(),
            tags: normalize_tags(tags),
            questions,
            created_at,
            updated_at: updated_at.unwrap_or(created_at),
        })
    }

    pub fn rename(&mut self, name: &str, now: DateTime<Utc>) {
        self.name = java_trim(name).to_string();
        self.updated_at = now;
    }

    pub fn update_tags<S: AsRef<str>>(
        &mut self,
        tags: impl IntoIterator<Item = S>,
        now: DateTime<Utc>,
    ) {
        self.tags = normalize_tags(tags);
        self.updated_at = now;
    }

    pub fn add_question(
        &mut self,
        text: &str,
        question_type: Option<&str>,
        now: DateTime<Utc>,
    ) -> AppResult<QuestionItem> {
        if self.questions.len() >= MAX_QUESTIONS_PER_GROUP {
            return Err(AppError::InvalidArgument(MAX_QUESTIONS_MESSAGE.to_string()));
        }
        let item = QuestionItem::create(text, question_type);
        self.questions.push(item.clone());
        self.updated_at = now;
        Ok(item)
    }

    pub fn update_question(
        &mut self,
        item_id: &QuestionItemId,
        text: &str,
        question_type: Option<&str>,
        now: DateTime<Utc>,
    ) {
        if let Some(existing) = self.questions.iter_mut().find(|q| q.id() == item_id) {
            *existing = existing.with_text_and_question_type(text, question_type);
            self.updated_at = now;
        }
    }

    pub fn remove_question(&mut self, item_id: &QuestionItemId, now: DateTime<Utc>) {
        self.questions.retain(|q| q.id() != item_id);
        self.updated_at = now;
    }

    pub fn has_any_tag<S: AsRef<str>>(&self, selected_tags: impl IntoIterator<Item = S>) -> bool {
        selected_tags
            .into_iter()
            .any(|tag| self.tags.contains(&normalize_tag(Some(tag.as_ref()))))
    }

    pub fn has_questions(&self) -> bool {
        !self.questions.is_empty()
    }

    pub fn id(&self) -> &QuestionGroupId {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn tags(&self) -> &[String] {
        &self.tags
    }

    pub fn questions(&self) -> &[QuestionItem] {
        &self.questions
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

/// 태그를 trim + 소문자로 만들고 빈 값과 중복을 제거한다(입력 순서 유지).
pub fn normalize_tags<S: AsRef<str>>(raw_tags: impl IntoIterator<Item = S>) -> Vec<String> {
    let mut normalized: Vec<String> = Vec::new();
    for raw in raw_tags {
        let tag = normalize_tag(Some(raw.as_ref()));
        if !java_is_blank(&tag) && !normalized.contains(&tag) {
            normalized.push(tag);
        }
    }
    normalized
}

pub fn normalize_tag(raw_tag: Option<&str>) -> String {
    raw_tag
        .map(|t| java_trim(t).to_lowercase())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::AppError;
    use chrono::{DateTime, Utc};
    use std::collections::HashSet;

    fn set(values: &[&str]) -> HashSet<String> {
        values.iter().map(|v| v.to_string()).collect()
    }

    fn tags_of(group: &QuestionGroupAggregate) -> HashSet<String> {
        group.tags().iter().cloned().collect()
    }

    #[test]
    fn create_normalizes_tags_trim_lowercase_deduplicate() {
        let group = QuestionGroupAggregate::create(
            "Travel Group",
            [" Travel ", "travel", "HABIT"],
            "2026-01-01T00:00:00Z".parse::<DateTime<Utc>>().unwrap(),
        );

        assert_eq!(set(&["travel", "habit"]), tags_of(&group));
        assert_eq!(2, group.tags().len());
    }

    #[test]
    fn add_question_allows_up_to_three_questions() {
        let mut group = QuestionGroupAggregate::create("g1", ["travel"], Utc::now());

        group.add_question("q1", Some("habit"), Utc::now()).unwrap();
        group
            .add_question("q2", Some("compare"), Utc::now())
            .unwrap();
        group.add_question("q3", None, Utc::now()).unwrap();

        assert_eq!(3, group.questions().len());
        assert!(matches!(
            group.add_question("q4", None, Utc::now()),
            Err(AppError::InvalidArgument(_))
        ));
    }

    #[test]
    fn update_question_updates_text_and_question_type() {
        let mut group = QuestionGroupAggregate::create("g2", ["habit"], Utc::now());
        let added = group
            .add_question("original", Some("habit"), Utc::now())
            .unwrap();

        group.update_question(added.id(), "updated", Some("compare"), Utc::now());

        assert_eq!("updated", group.questions()[0].text());
        assert_eq!(Some("compare"), group.questions()[0].question_type());
    }

    #[test]
    fn has_any_tag_returns_true_when_intersection_exists() {
        let group = QuestionGroupAggregate::create("g3", ["travel", "role-play"], Utc::now());

        assert!(group.has_any_tag(set(&["travel"])));
        assert!(!group.has_any_tag(set(&["habit"])));
    }
}

#[cfg(test)]
mod java_whitespace_tests {
    use super::*;

    #[test]
    fn normalize_tags_keeps_leading_ideographic_space_and_drops_blank_tag() {
        let tags = normalize_tags(["\u{3000}Home", "\u{3000}", "\u{00A0}"]);

        assert_eq!(vec!["\u{3000}home", "\u{00A0}"], tags);
    }

    #[test]
    fn create_keeps_leading_ideographic_space_in_name() {
        let group = QuestionGroupAggregate::create(
            "\u{3000}name \u{0001}",
            Vec::<String>::new(),
            chrono::Utc::now(),
        );

        assert_eq!("\u{3000}name", group.name());
    }
}
