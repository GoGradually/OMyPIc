//! 백업 가져오기의 "모두 바꾸기". 네 저장소와 룰북 색인을 SQLite 트랜잭션 하나로 바꾼다.

use std::sync::Arc;

use rusqlite::TransactionBehavior;

use omypic_application::datatransfer::port::{DataReplacement, DataReplacementPort};
use omypic_application::rulebook::port::EmbeddingPort;
use omypic_domain::error::AppResult;

use super::question::insert_group;
use super::rulebook::upsert_rulebook;
use super::rulebook_index::{embed_chunks, index_metadata, insert_chunks, write_metadata};
use super::wrongnote::{upsert_note, write_global_queue};
use super::{db_err, Database};

pub struct SqliteDataReplacementAdapter {
    db: Database,
    embedding: Arc<dyn EmbeddingPort>,
}

impl SqliteDataReplacementAdapter {
    pub fn new(db: Database, embedding: Arc<dyn EmbeddingPort>) -> Self {
        Self { db, embedding }
    }
}

impl DataReplacementPort for SqliteDataReplacementAdapter {
    /// 임베딩을 모두 계산한 뒤(실패하면 아무것도 바꾸지 않는다) 트랜잭션 하나에서 지우고 쓴다.
    /// 커밋 전에 프로세스가 죽으면 SQLite가 다음에 열 때 되돌린다.
    fn replace_all(&self, replacement: &DataReplacement) -> AppResult<()> {
        let embeddings = replacement
            .rulebooks
            .iter()
            .map(|r| embed_chunks(self.embedding.as_ref(), &r.chunks))
            .collect::<AppResult<Vec<_>>>()?;
        let metadata = index_metadata(self.embedding.as_ref());

        self.db.with_conn(|conn| {
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(db_err)?;
            tx.execute_batch(
                "DELETE FROM question_items;
                 DELETE FROM question_groups;
                 DELETE FROM rulebooks;
                 DELETE FROM wrong_notes;
                 DELETE FROM wrong_note_queue_states;
                 DELETE FROM rulebook_chunks;",
            )
            .map_err(db_err)?;
            for group in &replacement.question_groups {
                insert_group(&tx, group)?;
            }
            write_metadata(&tx, &metadata)?;
            for (item, blobs) in replacement.rulebooks.iter().zip(&embeddings) {
                upsert_rulebook(&tx, &item.rulebook)?;
                insert_chunks(
                    &tx,
                    item.rulebook.id(),
                    item.rulebook.filename(),
                    &item.chunks,
                    blobs,
                )?;
            }
            for note in &replacement.wrong_notes {
                upsert_note(&tx, note)?;
            }
            write_global_queue(&tx, &replacement.wrong_note_queue)?;
            tx.commit().map_err(db_err)
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::sync::atomic::{AtomicBool, Ordering};

    use chrono::{DateTime, Utc};
    use omypic_application::datatransfer::port::ReplacementRulebook;
    use omypic_application::question::port::QuestionGroupPort;
    use omypic_application::rulebook::port::{RulebookIndexPort, RulebookPort};
    use omypic_application::wrongnote::port::{WrongNotePort, WrongNoteRecentQueuePort};
    use omypic_domain::error::AppError;
    use omypic_domain::question::{
        QuestionGroupAggregate, QuestionGroupId, QuestionItem, QuestionItemId,
    };
    use omypic_domain::rulebook::{Rulebook, RulebookId, RulebookScope};
    use omypic_domain::wrongnote::{WrongNote, WrongNoteId};

    use super::*;
    use crate::sqlite::{
        SqliteQuestionGroupAdapter, SqliteRulebookAdapter, SqliteRulebookIndexAdapter,
        SqliteWrongNoteAdapter, SqliteWrongNoteRecentQueueAdapter,
    };

    /// 글자 수를 첫 성분으로 하는 2차원 임베딩. `available`이 거짓이면 실패한다.
    struct SwitchableEmbedding {
        available: AtomicBool,
    }

    impl EmbeddingPort for SwitchableEmbedding {
        fn embed(&self, text: &str) -> AppResult<Vec<f32>> {
            if self.available.load(Ordering::SeqCst) {
                Ok(vec![text.len() as f32, 1.0])
            } else {
                Err(AppError::InvalidArgument(
                    "Failed to download FastText model after retries".to_string(),
                ))
            }
        }
        fn dimension(&self) -> usize {
            2
        }
        fn provider(&self) -> String {
            "test".to_string()
        }
        fn model_version(&self) -> String {
            "v1".to_string()
        }
    }

    struct Stores {
        db: Database,
        embedding: Arc<SwitchableEmbedding>,
        groups: SqliteQuestionGroupAdapter,
        rulebooks: SqliteRulebookAdapter,
        notes: SqliteWrongNoteAdapter,
        queue: SqliteWrongNoteRecentQueueAdapter,
        index: SqliteRulebookIndexAdapter,
        replacement: SqliteDataReplacementAdapter,
    }

    fn stores(db: Database) -> Stores {
        let embedding = Arc::new(SwitchableEmbedding {
            available: AtomicBool::new(true),
        });
        Stores {
            groups: SqliteQuestionGroupAdapter::new(db.clone()),
            rulebooks: SqliteRulebookAdapter::new(db.clone()),
            notes: SqliteWrongNoteAdapter::new(db.clone()),
            queue: SqliteWrongNoteRecentQueueAdapter::new(db.clone()),
            index: SqliteRulebookIndexAdapter::new(db.clone(), embedding.clone()),
            replacement: SqliteDataReplacementAdapter::new(db.clone(), embedding.clone()),
            embedding,
            db,
        }
    }

    fn ts() -> DateTime<Utc> {
        "2026-02-01T00:00:00Z".parse().unwrap()
    }

    fn group(id: &str, item_ids: &[&str]) -> QuestionGroupAggregate {
        QuestionGroupAggregate::rehydrate(
            QuestionGroupId::of(id).unwrap(),
            id,
            ["tag"],
            item_ids
                .iter()
                .map(|item| QuestionItem::rehydrate(QuestionItemId::of(item).unwrap(), item, None))
                .collect(),
            Some(ts()),
            Some(ts()),
        )
        .unwrap()
    }

    fn rulebook(id: &str) -> Rulebook {
        Rulebook::rehydrate(
            RulebookId::of(id).unwrap(),
            &format!("{id}.md"),
            &format!("/data/rulebooks/{id}.md"),
            Some(RulebookScope::Main),
            None,
            true,
            Some(ts()),
            Some(ts()),
        )
        .unwrap()
    }

    fn note(id: &str) -> WrongNote {
        WrongNote::rehydrate(WrongNoteId::of(id).unwrap(), id, 2, "summary", Some(ts()))
    }

    fn chunk_count(db: &Database) -> i64 {
        db.with_conn(|c| {
            c.query_row("SELECT COUNT(*) FROM rulebook_chunks", [], |r| r.get(0))
                .map_err(db_err)
        })
        .unwrap()
    }

    fn seed(s: &Stores) {
        s.groups.save(group("old-group", &["oq"])).unwrap();
        s.rulebooks.save(rulebook("old-rulebook")).unwrap();
        s.index
            .index_rulebook_chunks(
                &RulebookId::of("old-rulebook").unwrap(),
                "old-rulebook.md",
                &["old chunk".to_string()],
            )
            .unwrap();
        s.notes.save(note("old-note")).unwrap();
        s.queue
            .save_global_queue(&["old-note".to_string()])
            .unwrap();
    }

    fn assert_seeded(s: &Stores) {
        let groups = s.groups.find_all().unwrap();
        assert_eq!(1, groups.len());
        assert_eq!("old-group", groups[0].id().value());
        assert_eq!(1, groups[0].questions().len());
        let rulebooks = s.rulebooks.find_all().unwrap();
        assert_eq!(1, rulebooks.len());
        assert_eq!("old-rulebook", rulebooks[0].id().value());
        let notes = s.notes.find_all().unwrap();
        assert_eq!(1, notes.len());
        assert_eq!("old-note", notes[0].id().value());
        assert_eq!(
            vec!["old-note".to_string()],
            s.queue.load_global_queue().unwrap()
        );
        assert_eq!(1, chunk_count(&s.db));
    }

    fn replacement(groups: Vec<QuestionGroupAggregate>) -> DataReplacement {
        DataReplacement {
            question_groups: groups,
            rulebooks: vec![
                ReplacementRulebook {
                    rulebook: rulebook("r1"),
                    chunks: vec!["a".to_string(), "bb".to_string()],
                },
                ReplacementRulebook {
                    rulebook: rulebook("r2"),
                    chunks: vec!["cccc".to_string()],
                },
            ],
            wrong_notes: vec![note("n1"), note("n2")],
            wrong_note_queue: vec!["n2".to_string(), "n1".to_string()],
        }
    }

    #[test]
    fn replace_all_replaces_every_store_and_the_index() {
        let s = stores(Database::open_in_memory().unwrap());
        seed(&s);

        s.replacement
            .replace_all(&replacement(vec![group("g1", &["q1"]), group("g2", &[])]))
            .unwrap();

        let group_ids: Vec<_> = s
            .groups
            .find_all()
            .unwrap()
            .iter()
            .map(|g| g.id().value().to_string())
            .collect();
        assert_eq!(vec!["g1", "g2"], group_ids);
        let rulebook_ids: Vec<_> = s
            .rulebooks
            .find_all()
            .unwrap()
            .iter()
            .map(|r| r.id().value().to_string())
            .collect();
        assert_eq!(vec!["r1", "r2"], rulebook_ids);
        assert_eq!(2, s.notes.find_all().unwrap().len());
        assert_eq!(
            vec!["n2".to_string(), "n1".to_string()],
            s.queue.load_global_queue().unwrap()
        );
        assert_eq!(3, chunk_count(&s.db));
        let enabled: HashSet<RulebookId> = ["r1", "r2", "old-rulebook"]
            .iter()
            .map(|id| RulebookId::of(id).unwrap())
            .collect();
        let found = s.index.search("cccc", 1, &enabled).unwrap();
        assert_eq!("cccc", found[0].text());
        assert_eq!("r2", found[0].rulebook_id().value());
    }

    #[test]
    fn replace_all_changes_nothing_when_embedding_fails() {
        let s = stores(Database::open_in_memory().unwrap());
        seed(&s);
        s.embedding.available.store(false, Ordering::SeqCst);

        let result = s
            .replacement
            .replace_all(&replacement(vec![group("g1", &[])]));

        assert_eq!(
            Err(AppError::InvalidArgument(
                "Failed to download FastText model after retries".to_string()
            )),
            result
        );
        s.embedding.available.store(true, Ordering::SeqCst);
        assert_seeded(&s);
    }

    #[test]
    fn replace_all_rolls_back_when_a_write_fails_and_nothing_is_left_after_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("omypic.db");
        let s = stores(Database::open(&path).unwrap());
        seed(&s);

        // 두 번째 그룹의 질문 id가 겹쳐 (group_id, id) 기본 키 위반으로 중간에 실패한다.
        let result = s.replacement.replace_all(&replacement(vec![
            group("g1", &["q1"]),
            group("g2", &["d", "d"]),
        ]));

        assert!(matches!(result, Err(AppError::Internal(_))), "{result:?}");
        assert_seeded(&s);
        drop(s);
        assert_seeded(&stores(Database::open(&path).unwrap()));
    }
}
