use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;
use omypic_domain::error::{AppError, AppResult};
use omypic_domain::question::{QuestionGroupAggregate, QuestionGroupId, QuestionItemId};
use omypic_domain::session::{SessionId, SessionState};

use super::model::{NextQuestion, QuestionTagStat};
use super::port::QuestionGroupPort;
use crate::session::port::{lock_session, SessionStorePort};
use crate::shared::port::MetricsPort;

pub struct QuestionUseCase {
    repository: Arc<dyn QuestionGroupPort>,
    session_store: Arc<dyn SessionStorePort>,
    metrics: Arc<dyn MetricsPort>,
}

impl QuestionUseCase {
    pub fn new(
        repository: Arc<dyn QuestionGroupPort>,
        session_store: Arc<dyn SessionStorePort>,
        metrics: Arc<dyn MetricsPort>,
    ) -> Self {
        Self {
            repository,
            session_store,
            metrics,
        }
    }

    pub fn list(&self) -> AppResult<Vec<QuestionGroupAggregate>> {
        self.repository.find_all()
    }

    pub fn create_group(&self, name: &str, tags: &[String]) -> AppResult<QuestionGroupAggregate> {
        let group = QuestionGroupAggregate::create(name, tags, Utc::now());
        self.repository.save(group)
    }

    pub fn update_group(
        &self,
        id: &str,
        name: &str,
        tags: &[String],
    ) -> AppResult<QuestionGroupAggregate> {
        let mut group = self.find_group(id)?;
        group.rename(name, Utc::now());
        group.update_tags(tags, Utc::now());
        self.repository.save(group)
    }

    pub fn delete_group(&self, id: &str) -> AppResult<()> {
        self.repository.delete_by_id(&QuestionGroupId::of(id)?)
    }

    pub fn add_question(
        &self,
        group_id: &str,
        text: &str,
        question_type: Option<&str>,
    ) -> AppResult<QuestionGroupAggregate> {
        let mut group = self.find_group(group_id)?;
        group.add_question(text, question_type, Utc::now())?;
        self.repository.save(group)
    }

    pub fn update_question(
        &self,
        group_id: &str,
        item_id: &str,
        text: &str,
        question_type: Option<&str>,
    ) -> AppResult<QuestionGroupAggregate> {
        let mut group = self.find_group(group_id)?;
        group.update_question(
            &QuestionItemId::of(item_id)?,
            text,
            question_type,
            Utc::now(),
        );
        self.repository.save(group)
    }

    pub fn delete_question(
        &self,
        group_id: &str,
        item_id: &str,
    ) -> AppResult<QuestionGroupAggregate> {
        let mut group = self.find_group(group_id)?;
        group.remove_question(&QuestionItemId::of(item_id)?, Utc::now());
        self.repository.save(group)
    }

    pub fn list_tag_stats(&self) -> AppResult<Vec<QuestionTagStat>> {
        let mut counts: HashMap<String, u64> = HashMap::new();
        for group in self.repository.find_all()? {
            for tag in group.tags() {
                *counts.entry(tag.clone()).or_insert(0) += 1;
            }
        }
        let mut stats: Vec<QuestionTagStat> = counts
            .into_iter()
            .map(|(tag, group_count)| QuestionTagStat {
                tag,
                group_count,
                selectable: group_count > 0,
            })
            .collect();
        // Java String 순서(UTF-16 코드 유닛)로 정렬한다.
        stats.sort_by(|a, b| a.tag.encode_utf16().cmp(b.tag.encode_utf16()));
        Ok(stats)
    }

    pub fn next_question(&self, session_id: &str) -> AppResult<NextQuestion> {
        let start = Instant::now();
        let session = self
            .session_store
            .get_or_create(&SessionId::of(session_id)?)?;
        let response = {
            let mut session = lock_session(&session)?;
            validate_selected_group_tags(&session)?;
            let groups_by_id = self.map_groups_by_id()?;
            select_next_question(&mut session, &groups_by_id)
        };
        self.metrics.record_question_next_latency(start.elapsed());
        Ok(response)
    }

    fn find_group(&self, id: &str) -> AppResult<QuestionGroupAggregate> {
        self.repository
            .find_by_id(&QuestionGroupId::of(id)?)?
            .ok_or_else(|| AppError::NotFound("No value present".to_string()))
    }

    fn map_groups_by_id(&self) -> AppResult<HashMap<String, QuestionGroupAggregate>> {
        let mut map = HashMap::new();
        for group in self.repository.find_all()? {
            map.entry(group.id().value().to_string()).or_insert(group);
        }
        Ok(map)
    }
}

fn validate_selected_group_tags(session: &SessionState) -> AppResult<()> {
    if session.selected_group_tags().is_empty() {
        return Err(AppError::InvalidState(
            "질문 그룹 태그를 선택하고 학습 모드를 적용해 주세요.".to_string(),
        ));
    }
    Ok(())
}

fn select_next_question(
    session: &mut SessionState,
    groups_by_id: &HashMap<String, QuestionGroupAggregate>,
) -> NextQuestion {
    loop {
        let Some(group_id) = session.current_candidate_group_id().map(str::to_string) else {
            return NextQuestion::skipped();
        };
        if let Some(response) = try_select_question(session, groups_by_id.get(&group_id)) {
            return response;
        }
        session.move_to_next_group();
    }
}

fn try_select_question(
    session: &mut SessionState,
    group: Option<&QuestionGroupAggregate>,
) -> Option<NextQuestion> {
    let group = group?;
    if group.questions().is_empty() {
        return None;
    }
    let group_id = group.id().value();
    let item = group
        .questions()
        .get(session.current_question_index(group_id))?;
    session.mark_question_asked(group_id);
    Some(NextQuestion {
        question_id: Some(item.id().value().to_string()),
        text: Some(item.text().to_string()),
        group_id: Some(group_id.to_string()),
        group: Some(group.name().to_string()),
        question_type: item.question_type().map(str::to_string),
        skipped: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        InMemoryQuestionGroupPort, InMemorySessionStorePort, RecordingMetrics,
    };
    use chrono::{TimeZone, Utc};
    use omypic_domain::error::AppError;
    use omypic_domain::question::{QuestionGroupId, QuestionItem, QuestionItemId};
    use omypic_domain::session::{SessionId, SessionState};

    struct Fixture {
        repository: Arc<InMemoryQuestionGroupPort>,
        session_store: Arc<InMemorySessionStorePort>,
        metrics: Arc<RecordingMetrics>,
        use_case: QuestionUseCase,
    }

    fn fixture(groups: Vec<QuestionGroupAggregate>) -> Fixture {
        let repository = Arc::new(InMemoryQuestionGroupPort::with_groups(groups));
        let session_store = Arc::new(InMemorySessionStorePort::new());
        let metrics = Arc::new(RecordingMetrics::new());
        let use_case =
            QuestionUseCase::new(repository.clone(), session_store.clone(), metrics.clone());
        Fixture {
            repository,
            session_store,
            metrics,
            use_case,
        }
    }

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| v.to_string()).collect()
    }

    fn group(
        id: &str,
        name: &str,
        tags: &[&str],
        questions: Vec<QuestionItem>,
    ) -> QuestionGroupAggregate {
        let at = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        QuestionGroupAggregate::rehydrate(
            QuestionGroupId::of(id).unwrap(),
            name,
            tags,
            questions,
            Some(at),
            Some(at),
        )
        .unwrap()
    }

    fn item(id: &str, text: &str, question_type: Option<&str>) -> QuestionItem {
        QuestionItem::rehydrate(QuestionItemId::of(id).unwrap(), text, question_type)
    }

    #[test]
    fn create_group_saves_group_with_normalized_tags() {
        let f = fixture(vec![]);

        let created = f
            .use_case
            .create_group("travel", &strings(&[" Travel ", "habit"]))
            .unwrap();

        assert_eq!("travel", created.name());
        assert!(created.tags().contains(&"travel".to_string()));
        assert!(created.tags().contains(&"habit".to_string()));
        assert_eq!(1, f.repository.save_calls());
        assert_eq!(1, f.repository.groups().len());
    }

    #[test]
    fn list_tag_stats_returns_sorted_counts() {
        let g1 = group(
            "g1",
            "A",
            &["travel", "habit"],
            vec![item("q1", "Q1", Some("habit"))],
        );
        let g2 = group(
            "g2",
            "B",
            &["travel"],
            vec![item("q2", "Q2", Some("compare"))],
        );
        let f = fixture(vec![g1, g2]);

        let stats = f.use_case.list_tag_stats().unwrap();

        assert_eq!(2, stats.len());
        assert_eq!("habit", stats[0].tag);
        assert_eq!(1, stats[0].group_count);
        assert!(stats[0].selectable);
        assert_eq!("travel", stats[1].tag);
        assert_eq!(2, stats[1].group_count);
    }

    #[test]
    fn next_question_uses_session_candidate_group_order_and_returns_skipped_when_exhausted() {
        let g1 = group(
            "g1",
            "Travel",
            &["travel"],
            vec![
                item("q1", "Q1", Some("habit")),
                item("q2", "Q2", Some("compare")),
            ],
        );
        let g2 = group("g2", "Cafe", &["travel"], vec![item("q3", "Q3", None)]);
        let f = fixture(vec![g1, g2]);
        let mut state = SessionState::new(SessionId::of("s1").unwrap());
        state.configure_question_groups(["travel"], ["g1", "g2"]);
        let handle = f.session_store.insert(state);

        let first = f.use_case.next_question("s1").unwrap();
        let second = f.use_case.next_question("s1").unwrap();
        let third = f.use_case.next_question("s1").unwrap();
        let fourth = f.use_case.next_question("s1").unwrap();

        assert_eq!(Some("q1".to_string()), first.question_id);
        assert_eq!(Some("g1".to_string()), first.group_id);
        assert_eq!(Some("habit".to_string()), first.question_type);
        assert_eq!(Some("q2".to_string()), second.question_id);
        assert_eq!(Some("q3".to_string()), third.question_id);
        assert!(fourth.skipped);
        assert_eq!(
            ["g1", "g2"],
            handle.lock().unwrap().candidate_group_order(),
            "candidateGroupOrder must remain fixed within a session"
        );
        assert_eq!(4, f.metrics.count("record_question_next_latency"));
    }

    #[test]
    fn next_question_throws_when_selected_tags_are_missing() {
        let f = fixture(vec![]);

        let error = f.use_case.next_question("s2").unwrap_err();

        assert!(matches!(error, AppError::InvalidState(_)));
    }
}
