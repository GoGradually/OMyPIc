use chrono::{DateTime, Utc};

use super::WrongNoteId;
use crate::shared::text_utils;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrongNote {
    id: WrongNoteId,
    pattern: String,
    count: i32,
    short_summary: String,
    last_seen_at: DateTime<Utc>,
}

impl WrongNote {
    pub fn create_new(pattern: &str, now: DateTime<Utc>) -> Self {
        Self::rehydrate(WrongNoteId::new_id(), pattern, 0, "", Some(now))
    }

    pub fn rehydrate(
        id: WrongNoteId,
        pattern: &str,
        count: i32,
        short_summary: &str,
        last_seen_at: Option<DateTime<Utc>>,
    ) -> Self {
        Self {
            id,
            pattern: pattern.to_string(),
            count,
            short_summary: short_summary.to_string(),
            last_seen_at: last_seen_at.unwrap_or_else(Utc::now),
        }
    }

    pub fn record_occurrence(
        &mut self,
        summary: &str,
        max_summary_chars: usize,
        now: DateTime<Utc>,
    ) {
        self.count += 1;
        self.short_summary = text_utils::trim_to_length(Some(summary), max_summary_chars);
        self.last_seen_at = now;
    }

    /// 횟수를 1 줄이고, 0 이하가 되었으면 `true`.
    pub fn decrement(&mut self) -> bool {
        self.count -= 1;
        self.count <= 0
    }

    pub fn id(&self) -> &WrongNoteId {
        &self.id
    }

    pub fn pattern(&self) -> &str {
        &self.pattern
    }

    pub fn count(&self) -> i32 {
        self.count
    }

    pub fn short_summary(&self) -> &str {
        &self.short_summary
    }

    pub fn last_seen_at(&self) -> DateTime<Utc> {
        self.last_seen_at
    }
}
