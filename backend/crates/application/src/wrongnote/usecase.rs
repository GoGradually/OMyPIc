use std::sync::{Arc, Mutex};

use chrono::Utc;
use omypic_domain::error::AppResult;
use omypic_domain::feedback::{CorrectionDetail, Feedback, RecommendationDetail};
use omypic_domain::shared::text_utils::{java_is_blank, trim_to_length};
use omypic_domain::wrongnote::{WrongNote, WrongNoteWindow};

use super::port::{WrongNotePort, WrongNoteRecentQueuePort};
use crate::feedback::policy::FeedbackPolicy;

const MAX_PATTERN_CHARS: usize = 120;

pub struct WrongNoteUseCase {
    repository: Arc<dyn WrongNotePort>,
    recent_queue_store: Arc<dyn WrongNoteRecentQueuePort>,
    feedback_policy: Arc<dyn FeedbackPolicy>,
    /// Java의 `synchronized addFeedback`.
    add_lock: Mutex<()>,
}

impl WrongNoteUseCase {
    pub fn new(
        repository: Arc<dyn WrongNotePort>,
        recent_queue_store: Arc<dyn WrongNoteRecentQueuePort>,
        feedback_policy: Arc<dyn FeedbackPolicy>,
    ) -> Self {
        Self {
            repository,
            recent_queue_store,
            feedback_policy,
            add_lock: Mutex::new(()),
        }
    }

    pub fn add_feedback(&self, response: &Feedback) -> AppResult<()> {
        let _guard = self
            .add_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut window = self.load_window()?;
        let corrections = response.corrections();
        let recommendations = response.recommendations();
        let points = [
            correction_point("Grammar", corrections.grammar()),
            correction_point("Expression", corrections.expression()),
            correction_point("Logic", corrections.logic()),
            recommendation_point("Filler", recommendations.filler()),
            recommendation_point("Adjective", recommendations.adjective()),
            recommendation_point("Adverb", recommendations.adverb()),
        ];
        for point in points.iter().filter(|point| !java_is_blank(point)) {
            self.apply_correction_point(&mut window, point)?;
        }
        self.recent_queue_store
            .save_global_queue(&window.snapshot())
    }

    pub fn list(&self) -> AppResult<Vec<WrongNote>> {
        let mut notes = self.repository.find_all()?;
        notes.sort_by_key(|note| std::cmp::Reverse(note.count()));
        Ok(notes)
    }

    fn load_window(&self) -> AppResult<WrongNoteWindow> {
        WrongNoteWindow::from(
            self.recent_queue_store.load_global_queue()?,
            self.feedback_policy.get_wrongnote_window_size(),
        )
    }

    fn apply_correction_point(&self, window: &mut WrongNoteWindow, point: &str) -> AppResult<()> {
        let pattern = trim_to_length(Some(point), MAX_PATTERN_CHARS);
        let mut note = match self.repository.find_by_pattern(&pattern)? {
            Some(note) => note,
            None => WrongNote::create_new(&pattern, Utc::now()),
        };
        note.record_occurrence(
            point,
            self.feedback_policy.get_wrongnote_summary_max_chars(),
            Utc::now(),
        );
        self.repository.save(note)?;
        match window.append(&pattern) {
            Some(evicted) => self.decrement_or_remove(&evicted),
            None => Ok(()),
        }
    }

    fn decrement_or_remove(&self, pattern: &str) -> AppResult<()> {
        let Some(mut note) = self.repository.find_by_pattern(pattern)? else {
            return Ok(());
        };
        if note.decrement() {
            self.repository.delete_by_id(note.id())
        } else {
            self.repository.save(note).map(|_| ())
        }
    }
}

fn correction_point(category: &str, detail: &CorrectionDetail) -> String {
    join_point(category, detail.issue(), detail.fix(), " | ")
}

fn recommendation_point(category: &str, detail: &RecommendationDetail) -> String {
    join_point(category, detail.term(), detail.usage(), " - ")
}

/// 두 값이 모두 있으면 `category: first<separator>second`, 하나만 있으면 `category: 그 값`.
fn join_point(category: &str, first: &str, second: &str, separator: &str) -> String {
    match (java_is_blank(first), java_is_blank(second)) {
        (false, false) => format!("{category}: {first}{separator}{second}"),
        (false, true) => format!("{category}: {first}"),
        (true, false) => format!("{category}: {second}"),
        (true, true) => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        FakeFeedbackPolicy, InMemoryWrongNotePort, InMemoryWrongNoteRecentQueuePort,
    };
    use omypic_domain::feedback::{
        CorrectionDetail, Corrections, RecommendationDetail, Recommendations,
    };

    struct Fixture {
        repository: Arc<InMemoryWrongNotePort>,
        queue: Arc<InMemoryWrongNoteRecentQueuePort>,
        use_case: WrongNoteUseCase,
    }

    fn fixture(window_size: usize) -> Fixture {
        let repository = Arc::new(InMemoryWrongNotePort::new());
        let queue = Arc::new(InMemoryWrongNoteRecentQueuePort::new());
        let policy = Arc::new(FakeFeedbackPolicy {
            wrongnote_summary_max_chars: 255,
            wrongnote_window_size: window_size,
            ..FakeFeedbackPolicy::default()
        });
        let use_case = WrongNoteUseCase::new(repository.clone(), queue.clone(), policy);
        Fixture {
            repository,
            queue,
            use_case,
        }
    }

    fn feedback_with_single_pattern(issue: &str) -> Feedback {
        Feedback::of(
            "summary",
            Corrections::new(
                CorrectionDetail::new(issue, ""),
                CorrectionDetail::new("", ""),
                CorrectionDetail::new("", ""),
            ),
            Recommendations::new(
                RecommendationDetail::new("", ""),
                RecommendationDetail::new("", ""),
                RecommendationDetail::new("", ""),
            ),
            "",
            &[],
        )
    }

    #[test]
    fn add_feedback_increments_duplicate_pattern_count() {
        let f = fixture(100);

        f.use_case
            .add_feedback(&feedback_with_single_pattern("tense"))
            .unwrap();
        f.use_case
            .add_feedback(&feedback_with_single_pattern("tense"))
            .unwrap();

        let note = f.repository.note_by_pattern("Grammar: tense").unwrap();
        assert_eq!(2, note.count());
    }

    #[test]
    fn add_feedback_trims_pattern_to_120_chars() {
        let f = fixture(100);
        let long_point = "x".repeat(150);

        f.use_case
            .add_feedback(&feedback_with_single_pattern(&long_point))
            .unwrap();

        let saved = f.repository.notes();
        assert_eq!(1, saved.len());
        assert_eq!(119, saved[0].pattern().chars().count());
    }

    #[test]
    fn add_feedback_keeps_recent_queue_at_100_and_deletes_zero_count_notes() {
        let f = fixture(100);

        for i in 0..101 {
            f.use_case
                .add_feedback(&feedback_with_single_pattern(&format!("pattern-{i}")))
                .unwrap();
        }

        assert!(f.repository.note_by_pattern("Grammar: pattern-0").is_none());
        assert_eq!(100, f.repository.notes().len());
    }

    #[test]
    fn add_feedback_applies_configured_window_size() {
        let f = fixture(2);

        for issue in ["A", "B", "C"] {
            f.use_case
                .add_feedback(&feedback_with_single_pattern(issue))
                .unwrap();
        }

        assert_eq!(vec!["Grammar: B", "Grammar: C"], f.queue.queue());
        assert!(f.repository.note_by_pattern("Grammar: A").is_none());
    }

    #[test]
    fn add_feedback_decrements_existing_count_when_evicted_from_queue() {
        let f = fixture(100);

        f.use_case
            .add_feedback(&feedback_with_single_pattern("A"))
            .unwrap();
        f.use_case
            .add_feedback(&feedback_with_single_pattern("A"))
            .unwrap();
        for i in 0..99 {
            f.use_case
                .add_feedback(&feedback_with_single_pattern(&format!("B-{i}")))
                .unwrap();
        }

        let note = f.repository.note_by_pattern("Grammar: A").unwrap();
        assert_eq!(1, note.count());
    }

    #[test]
    fn list_returns_descending_by_count() {
        let f = fixture(100);

        for issue in ["A", "A", "A", "B"] {
            f.use_case
                .add_feedback(&feedback_with_single_pattern(issue))
                .unwrap();
        }
        let notes = f.use_case.list().unwrap();

        assert_eq!("Grammar: A", notes[0].pattern());
        assert_eq!(3, notes[0].count());
        assert_eq!(1, notes[1].count());
    }

    #[test]
    fn add_feedback_saves_queue_state() {
        let f = fixture(100);

        f.use_case
            .add_feedback(&feedback_with_single_pattern("A"))
            .unwrap();

        assert_eq!(1, f.queue.save_calls());
        assert_eq!(vec!["Grammar: A"], f.queue.queue());
    }

    #[test]
    fn add_feedback_records_recommendation_points() {
        let f = fixture(100);

        f.use_case
            .add_feedback(&Feedback::of_points(
                "summary",
                &[
                    "Grammar: tense".to_string(),
                    "Expression: clearer wording".to_string(),
                    "Logic: add reason".to_string(),
                ],
                &[
                    "Filler: Well: Use this to start naturally.".to_string(),
                    "Adjective: impressive - Use it for vivid detail.".to_string(),
                    "Adverb: definitely - Use it for confidence.".to_string(),
                ],
                "",
                &[],
            ))
            .unwrap();

        for pattern in [
            "Filler: Well: Use this to start naturally.",
            "Adjective: impressive - Use it for vivid detail.",
            "Adverb: definitely - Use it for confidence.",
        ] {
            let note = f.repository.note_by_pattern(pattern).unwrap();
            assert_eq!(1, note.count());
        }
    }
}
