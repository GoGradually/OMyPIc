use std::collections::HashSet;
use std::sync::Arc;

use omypic_domain::error::{AppError, AppResult};
use omypic_domain::question::{normalize_tags, QuestionGroupAggregate};
use omypic_domain::session::SessionId;
use uuid::Uuid;

use super::model::ModeUpdateCommand;
use super::port::{lock_session, SessionStorePort, SharedSessionState};
use crate::question::port::QuestionGroupPort;

pub struct SessionUseCase {
    session_store: Arc<dyn SessionStorePort>,
    question_group_port: Arc<dyn QuestionGroupPort>,
}

impl SessionUseCase {
    pub fn new(
        session_store: Arc<dyn SessionStorePort>,
        question_group_port: Arc<dyn QuestionGroupPort>,
    ) -> Self {
        Self {
            session_store,
            question_group_port,
        }
    }

    pub fn get_or_create(&self, session_id: &str) -> AppResult<SharedSessionState> {
        self.session_store
            .get_or_create(&SessionId::of(session_id)?)
    }

    /// 모드와 질문 그룹 태그를 적용한다. 모드 변경은 태그 검증보다 먼저 적용된다(Java와 같다).
    pub fn update_mode(&self, command: &ModeUpdateCommand) -> AppResult<SharedSessionState> {
        let state = self.get_or_create(&command.session_id)?;
        lock_session(&state).apply_mode_update(command.mode, command.continuous_batch_size);
        let selected_tags = normalize_tags(&command.selected_group_tags);
        if selected_tags.is_empty() {
            return Err(AppError::InvalidGroupTags {
                message: "selectedGroupTags must not be empty".to_string(),
                invalid_tags: vec![],
            });
        }
        let all_groups = self.question_group_port.find_all()?;
        validate_selected_tags(&selected_tags, &all_groups)?;
        let candidate_group_ids = shuffled_candidate_group_ids(&all_groups, &selected_tags);
        lock_session(&state).configure_question_groups(&selected_tags, &candidate_group_ids);
        Ok(state)
    }

    pub fn append_segment(&self, session_id: &str, text: &str) -> AppResult<()> {
        let state = self.get_or_create(session_id)?;
        lock_session(&state).append_segment(text);
        Ok(())
    }
}

fn validate_selected_tags(
    selected_tags: &[String],
    all_groups: &[QuestionGroupAggregate],
) -> AppResult<()> {
    let available: HashSet<&str> = all_groups
        .iter()
        .flat_map(|group| group.tags().iter().map(String::as_str))
        .collect();
    let mut invalid: Vec<String> = selected_tags
        .iter()
        .filter(|tag| !available.contains(tag.as_str()))
        .cloned()
        .collect();
    if invalid.is_empty() {
        return Ok(());
    }
    // Java String 순서(UTF-16 코드 유닛)로 정렬한다.
    invalid.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
    Err(AppError::InvalidGroupTags {
        message: "Some selectedGroupTags are invalid".to_string(),
        invalid_tags: invalid,
    })
}

fn shuffled_candidate_group_ids(
    all_groups: &[QuestionGroupAggregate],
    selected_tags: &[String],
) -> Vec<String> {
    let mut ids: Vec<String> = all_groups
        .iter()
        .filter(|group| group.has_any_tag(selected_tags))
        .filter(|group| group.has_questions())
        .map(|group| group.id().value().to_string())
        .collect();
    shuffle(&mut ids);
    ids
}

/// Fisher-Yates. 난수는 UUID v4의 무작위 비트에서 얻는다(암호용이 아니다).
fn shuffle<T>(items: &mut [T]) {
    for i in (1..items.len()).rev() {
        let bytes = Uuid::new_v4().as_u128();
        let j = (bytes % (i as u128 + 1)) as usize;
        items.swap(i, j);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{InMemoryQuestionGroupPort, InMemorySessionStorePort};
    use chrono::{TimeZone, Utc};
    use omypic_domain::error::AppError;
    use omypic_domain::question::{
        QuestionGroupAggregate, QuestionGroupId, QuestionItem, QuestionItemId,
    };
    use omypic_domain::session::{ModeType, SessionId, SessionState};

    struct Fixture {
        store: Arc<InMemorySessionStorePort>,
        groups: Arc<InMemoryQuestionGroupPort>,
        use_case: SessionUseCase,
    }

    fn fixture(groups: Vec<QuestionGroupAggregate>) -> Fixture {
        let store = Arc::new(InMemorySessionStorePort::new());
        let groups = Arc::new(InMemoryQuestionGroupPort::with_groups(groups));
        let use_case = SessionUseCase::new(store.clone(), groups.clone());
        Fixture {
            store,
            groups,
            use_case,
        }
    }

    fn command(
        session_id: &str,
        mode: ModeType,
        batch: Option<i32>,
        selected_tags: &[&str],
    ) -> ModeUpdateCommand {
        ModeUpdateCommand {
            session_id: session_id.to_string(),
            mode: Some(mode),
            continuous_batch_size: batch,
            selected_group_tags: selected_tags.iter().map(|t| t.to_string()).collect(),
        }
    }

    fn group(id: &str, tags: &[&str], with_questions: bool) -> QuestionGroupAggregate {
        let questions = if with_questions {
            vec![QuestionItem::rehydrate(
                QuestionItemId::of(&format!("q-{id}")).unwrap(),
                "Q",
                None,
            )]
        } else {
            vec![]
        };
        let at = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        QuestionGroupAggregate::rehydrate(
            QuestionGroupId::of(id).unwrap(),
            id,
            tags,
            questions,
            Some(at),
            Some(at),
        )
        .unwrap()
    }

    fn state(f: &Fixture, id: &str) -> SharedSessionState {
        f.store
            .insert(SessionState::new(SessionId::of(id).unwrap()))
    }

    #[test]
    fn update_mode_clamps_continuous_batch_size_between_one_and_ten() {
        let f = fixture(vec![group("g1", &["travel"], true)]);
        let handle = state(&f, "s1");

        f.use_case
            .update_mode(&command("s1", ModeType::Continuous, Some(100), &["travel"]))
            .unwrap();
        assert_eq!(10, handle.lock().unwrap().continuous_batch_size());

        f.use_case
            .update_mode(&command("s1", ModeType::Continuous, Some(0), &["travel"]))
            .unwrap();
        assert_eq!(1, handle.lock().unwrap().continuous_batch_size());
    }

    #[test]
    fn update_mode_configures_candidate_groups_for_selected_tags() {
        let f = fixture(vec![
            group("g1", &["travel"], true),
            group("g2", &["habit"], true),
            group("g3", &["travel"], false),
        ]);
        state(&f, "s2");

        let updated = f
            .use_case
            .update_mode(&command("s2", ModeType::Immediate, None, &[" travel "]))
            .unwrap();

        let updated = updated.lock().unwrap();
        assert_eq!(["travel"], updated.selected_group_tags());
        assert_eq!(1, updated.candidate_group_order().len());
        assert_eq!("g1", updated.candidate_group_order()[0]);
    }

    #[test]
    fn update_mode_throws_when_selected_tags_are_empty() {
        let f = fixture(vec![]);
        state(&f, "s3");

        let error = f
            .use_case
            .update_mode(&command("s3", ModeType::Immediate, None, &[]))
            .unwrap_err();

        assert_eq!(
            AppError::InvalidGroupTags {
                message: "selectedGroupTags must not be empty".to_string(),
                invalid_tags: vec![]
            },
            error
        );
        assert_eq!(0, f.groups.find_all_calls());
    }

    #[test]
    fn update_mode_throws_when_unknown_tags_are_included() {
        let f = fixture(vec![group("g1", &["travel"], true)]);
        state(&f, "s4");

        let error = f
            .use_case
            .update_mode(&command(
                "s4",
                ModeType::Immediate,
                None,
                &["travel", "unknown"],
            ))
            .unwrap_err();

        assert_eq!(
            AppError::InvalidGroupTags {
                message: "Some selectedGroupTags are invalid".to_string(),
                invalid_tags: vec!["unknown".to_string()]
            },
            error
        );
    }

    #[test]
    fn append_segment_appends_text_to_session() {
        let f = fixture(vec![]);
        let handle = state(&f, "s5");

        f.use_case.append_segment("s5", "hello").unwrap();

        assert_eq!(vec!["hello"], handle.lock().unwrap().stt_segments());
    }
}

#[cfg(test)]
mod shuffle_tests {
    use super::shuffle;

    #[test]
    fn shuffle_keeps_every_element() {
        let mut items: Vec<u32> = (0..50).collect();

        shuffle(&mut items);

        let mut sorted = items.clone();
        sorted.sort();
        assert_eq!((0..50).collect::<Vec<u32>>(), sorted);
    }

    #[test]
    fn shuffle_handles_empty_and_single_element() {
        let mut empty: Vec<u32> = vec![];
        let mut single = vec![1];

        shuffle(&mut empty);
        shuffle(&mut single);

        assert!(empty.is_empty());
        assert_eq!(vec![1], single);
    }
}
