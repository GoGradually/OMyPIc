//! SQLite 영속성 어댑터(MongoDB 대체).

mod question;
mod rulebook;
pub mod schema;
mod wrongnote;

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{DateTime, SecondsFormat, Utc};
use rusqlite::Connection;

use omypic_domain::error::{AppError, AppResult};

pub use question::SqliteQuestionGroupAdapter;
pub use rulebook::SqliteRulebookAdapter;
pub use wrongnote::{SqliteWrongNoteAdapter, SqliteWrongNoteRecentQueueAdapter};

/// 연결 하나를 뮤텍스로 공유하는 SQLite 핸들. 복제해도 같은 연결을 쓴다.
#[derive(Clone)]
pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

impl Database {
    pub fn open(path: &Path) -> AppResult<Database> {
        let conn = Connection::open(path).map_err(db_err)?;
        Self::init(conn, true)
    }

    pub fn open_in_memory() -> AppResult<Database> {
        let conn = Connection::open_in_memory().map_err(db_err)?;
        Self::init(conn, false)
    }

    fn init(conn: Connection, wal: bool) -> AppResult<Database> {
        conn.busy_timeout(Duration::from_secs(5)).map_err(db_err)?;
        if wal {
            conn.query_row("PRAGMA journal_mode=WAL", [], |row| row.get::<_, String>(0))
                .map_err(db_err)?;
        }
        conn.execute_batch("PRAGMA foreign_keys=ON")
            .map_err(db_err)?;
        schema::migrate(&conn)?;
        Ok(Database {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// 연결을 잠그고 `f`를 실행한다. 다른 스레드가 패닉해 뮤텍스가 오염돼도
    /// 연결은 그대로 쓴다(진행 중이던 트랜잭션은 연결을 닫을 때 롤백된다).
    pub fn with_conn<R>(&self, f: impl FnOnce(&mut Connection) -> AppResult<R>) -> AppResult<R> {
        let mut guard = self
            .conn
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        f(&mut guard)
    }
}

pub(crate) fn db_err(err: rusqlite::Error) -> AppError {
    AppError::Internal(err.to_string())
}

/// 나노초까지 보존하는 고정 폭 ISO-8601(UTC) 문자열.
pub(crate) fn format_ts(ts: DateTime<Utc>) -> String {
    ts.to_rfc3339_opts(SecondsFormat::Nanos, true)
}

pub(crate) fn parse_ts(text: &str) -> AppResult<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(text)
        .map(|ts| ts.with_timezone(&Utc))
        .map_err(|err| AppError::Internal(format!("Invalid stored timestamp '{text}': {err}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use omypic_application::question::port::QuestionGroupPort;
    use omypic_application::wrongnote::port::WrongNotePort;
    use omypic_domain::question::{
        QuestionGroupAggregate, QuestionGroupId, QuestionItem, QuestionItemId,
    };
    use omypic_domain::wrongnote::{WrongNote, WrongNoteId};

    fn user_version(db: &Database) -> i64 {
        db.with_conn(|c| {
            c.query_row("PRAGMA user_version", [], |r| r.get(0))
                .map_err(db_err)
        })
        .unwrap()
    }

    #[test]
    fn migrate_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("omypic.db");
        let db = Database::open(&path).unwrap();
        let note = WrongNote::rehydrate(
            WrongNoteId::of("n1").unwrap(),
            "p",
            1,
            "s",
            Some(Utc::now()),
        );
        SqliteWrongNoteAdapter::new(db.clone()).save(note).unwrap();
        drop(db);

        let db = Database::open(&path).unwrap();
        assert_eq!(1, user_version(&db));
        let notes = SqliteWrongNoteAdapter::new(db).find_all().unwrap();
        assert_eq!(1, notes.len());
        assert_eq!("n1", notes[0].id().value());
    }

    #[test]
    fn open_rejects_newer_schema() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("omypic.db");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch("PRAGMA user_version = 99").unwrap();
        drop(conn);

        let err = Database::open(&path).err().expect("must fail");
        assert_eq!(
            AppError::InvalidState("Unsupported database version: 99".to_string()),
            err
        );
    }

    #[test]
    fn timestamps_keep_nanosecond_precision() {
        let ts: DateTime<Utc> = "2026-02-01T03:04:05.123456789Z".parse().unwrap();
        assert_eq!(ts, parse_ts(&format_ts(ts)).unwrap());

        let db = Database::open_in_memory().unwrap();
        let note = WrongNote::rehydrate(WrongNoteId::of("n1").unwrap(), "p", 1, "s", Some(ts));
        let adapter = SqliteWrongNoteAdapter::new(db);
        adapter.save(note).unwrap();
        assert_eq!(ts, adapter.find_all().unwrap()[0].last_seen_at());
    }

    #[test]
    fn concurrent_writers_and_readers_do_not_fail() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("omypic.db")).unwrap();
        let groups = SqliteQuestionGroupAdapter::new(db.clone());
        let notes = SqliteWrongNoteAdapter::new(db);
        let group = QuestionGroupAggregate::create("g", ["t"], Utc::now());
        groups.save(group).unwrap();

        let errors = std::sync::atomic::AtomicUsize::new(0);
        std::thread::scope(|scope| {
            for writer in 0..8 {
                let notes = &notes;
                let errors = &errors;
                scope.spawn(move || {
                    for n in 0..50 {
                        let note = WrongNote::rehydrate(
                            WrongNoteId::of(&format!("w{writer}-{n}")).unwrap(),
                            &format!("pattern-{writer}-{n}"),
                            1,
                            "s",
                            Some(Utc::now()),
                        );
                        if notes.save(note).is_err() {
                            errors.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        }
                    }
                });
            }
            for _ in 0..4 {
                let groups = &groups;
                let errors = &errors;
                scope.spawn(move || {
                    for _ in 0..200 {
                        match groups.find_all() {
                            Ok(found) if found.len() == 1 => {}
                            _ => {
                                errors.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                            }
                        }
                    }
                });
            }
        });

        assert_eq!(0, errors.load(std::sync::atomic::Ordering::SeqCst));
        assert_eq!(400, notes.find_all().unwrap().len());
    }

    #[test]
    fn failed_group_save_keeps_previous_group() {
        let db = Database::open_in_memory().unwrap();
        let adapter = SqliteQuestionGroupAdapter::new(db);
        let id = QuestionGroupId::of("g1").unwrap();
        let ts: DateTime<Utc> = "2026-02-01T00:00:00Z".parse().unwrap();
        let item = |id: &str, text: &str| {
            QuestionItem::rehydrate(QuestionItemId::of(id).unwrap(), text, None)
        };
        let original = QuestionGroupAggregate::rehydrate(
            id.clone(),
            "Original",
            ["a"],
            vec![item("q1", "first")],
            Some(ts),
            Some(ts),
        )
        .unwrap();
        adapter.save(original).unwrap();

        let broken = QuestionGroupAggregate::rehydrate(
            id.clone(),
            "Broken",
            ["b"],
            vec![item("dup", "x"), item("dup", "y")],
            Some(ts),
            Some(ts),
        )
        .unwrap();
        let err = adapter.save(broken).unwrap_err();
        assert!(matches!(err, AppError::Internal(_)));

        let stored = adapter.find_by_id(&id).unwrap().unwrap();
        assert_eq!("Original", stored.name());
        assert_eq!(["a".to_string()], stored.tags());
        assert_eq!(1, stored.questions().len());
        assert_eq!("first", stored.questions()[0].text());
    }
}
