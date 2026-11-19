use rusqlite::{params, Connection, OptionalExtension};

use omypic_application::wrongnote::port::{WrongNotePort, WrongNoteRecentQueuePort};
use omypic_domain::error::{AppError, AppResult};
use omypic_domain::wrongnote::{WrongNote, WrongNoteId};

use super::{db_err, format_ts, parse_ts, Database};

pub struct SqliteWrongNoteAdapter {
    db: Database,
}

impl SqliteWrongNoteAdapter {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

const COLUMNS: &str = "id, pattern, count, short_summary, last_seen_at";

fn to_domain(row: &rusqlite::Row<'_>) -> rusqlite::Result<AppResult<WrongNote>> {
    let id: String = row.get(0)?;
    let pattern: String = row.get(1)?;
    let count: i32 = row.get(2)?;
    let short_summary: String = row.get(3)?;
    let last_seen_at: String = row.get(4)?;
    Ok((|| {
        Ok(WrongNote::rehydrate(
            WrongNoteId::of(&id)?,
            &pattern,
            count,
            &short_summary,
            Some(parse_ts(&last_seen_at)?),
        ))
    })())
}

fn find_by_id(conn: &Connection, id: &str) -> AppResult<Option<WrongNote>> {
    conn.query_row(
        &format!("SELECT {COLUMNS} FROM wrong_notes WHERE id = ?1"),
        params![id],
        to_domain,
    )
    .optional()
    .map_err(db_err)?
    .transpose()
}

pub(super) fn upsert_note(conn: &Connection, note: &WrongNote) -> AppResult<()> {
    conn.execute(
        &format!(
            "INSERT INTO wrong_notes ({COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET pattern = excluded.pattern,
                 count = excluded.count, short_summary = excluded.short_summary,
                 last_seen_at = excluded.last_seen_at"
        ),
        params![
            note.id().value(),
            note.pattern(),
            note.count(),
            note.short_summary(),
            format_ts(note.last_seen_at()),
        ],
    )
    .map(|_| ())
    .map_err(db_err)
}

impl WrongNotePort for SqliteWrongNoteAdapter {
    /// 저장된 순서(rowid)대로. Mongo의 `findAll`도 정렬하지 않았다.
    fn find_all(&self) -> AppResult<Vec<WrongNote>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn
                .prepare(&format!("SELECT {COLUMNS} FROM wrong_notes ORDER BY rowid"))
                .map_err(db_err)?;
            let rows = stmt
                .query_map([], to_domain)
                .map_err(db_err)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(db_err)?;
            rows.into_iter().collect()
        })
    }

    /// pattern은 유일하지 않을 수 있다(Mongo에도 유니크 인덱스가 없었다). 먼저 저장된 행을 돌려준다.
    fn find_by_pattern(&self, pattern: &str) -> AppResult<Option<WrongNote>> {
        self.db.with_conn(|conn| {
            conn.query_row(
                &format!(
                    "SELECT {COLUMNS} FROM wrong_notes WHERE pattern = ?1 ORDER BY rowid LIMIT 1"
                ),
                params![pattern],
                to_domain,
            )
            .optional()
            .map_err(db_err)?
            .transpose()
        })
    }

    fn save(&self, note: WrongNote) -> AppResult<WrongNote> {
        self.db.with_conn(|conn| {
            upsert_note(conn, &note)?;
            find_by_id(conn, note.id().value())?
                .ok_or_else(|| AppError::Internal("Saved wrong note not found".to_string()))
        })
    }

    fn delete_by_id(&self, id: &WrongNoteId) -> AppResult<()> {
        self.db.with_conn(|conn| {
            conn.execute("DELETE FROM wrong_notes WHERE id = ?1", params![id.value()])
                .map(|_| ())
                .map_err(db_err)
        })
    }

    fn delete_all(&self) -> AppResult<()> {
        self.db.with_conn(|conn| {
            conn.execute("DELETE FROM wrong_notes", [])
                .map(|_| ())
                .map_err(db_err)
        })
    }
}

const GLOBAL_ID: &str = "global";
const MAX_PATTERNS: usize = 100;

/// 전역 큐를 쓴다. 마지막 100개만 저장한다.
pub(super) fn write_global_queue(conn: &Connection, patterns: &[String]) -> AppResult<()> {
    let kept = &patterns[patterns.len().saturating_sub(MAX_PATTERNS)..];
    let json = serde_json::to_string(kept).map_err(|err| AppError::Internal(err.to_string()))?;
    conn.execute(
        "INSERT INTO wrong_note_queue_states (id, patterns_json, updated_at)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(id) DO UPDATE SET patterns_json = excluded.patterns_json,
             updated_at = excluded.updated_at",
        params![GLOBAL_ID, json, format_ts(chrono::Utc::now())],
    )
    .map(|_| ())
    .map_err(db_err)
}

pub struct SqliteWrongNoteRecentQueueAdapter {
    db: Database,
}

impl SqliteWrongNoteRecentQueueAdapter {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

impl WrongNoteRecentQueuePort for SqliteWrongNoteRecentQueueAdapter {
    fn load_global_queue(&self) -> AppResult<Vec<String>> {
        self.db.with_conn(|conn| {
            let json: Option<String> = conn
                .query_row(
                    "SELECT patterns_json FROM wrong_note_queue_states WHERE id = ?1",
                    params![GLOBAL_ID],
                    |row| row.get(0),
                )
                .optional()
                .map_err(db_err)?;
            match json {
                None => Ok(Vec::new()),
                Some(json) => serde_json::from_str(&json)
                    .map_err(|err| AppError::Internal(format!("Invalid stored queue: {err}"))),
            }
        })
    }

    /// 마지막 100개만 저장한다.
    fn save_global_queue(&self, patterns: &[String]) -> AppResult<()> {
        self.db.with_conn(|conn| write_global_queue(conn, patterns))
    }

    fn clear_global_queue(&self) -> AppResult<()> {
        self.db.with_conn(|conn| {
            conn.execute(
                "DELETE FROM wrong_note_queue_states WHERE id = ?1",
                params![GLOBAL_ID],
            )
            .map(|_| ())
            .map_err(db_err)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};

    fn ts() -> DateTime<Utc> {
        "2026-02-01T00:00:00Z".parse().unwrap()
    }

    fn note(id: &str, pattern: &str, count: i32) -> WrongNote {
        WrongNote::rehydrate(
            WrongNoteId::of(id).unwrap(),
            pattern,
            count,
            "summary",
            Some(ts()),
        )
    }

    fn adapter() -> SqliteWrongNoteAdapter {
        SqliteWrongNoteAdapter::new(Database::open_in_memory().unwrap())
    }

    #[test]
    fn find_all_maps_rows_to_domain() {
        let adapter = adapter();
        adapter.save(note("n1", "pattern", 2)).unwrap();

        let result = adapter.find_all().unwrap();

        assert_eq!(1, result.len());
        assert_eq!("n1", result[0].id().value());
        assert_eq!("pattern", result[0].pattern());
        assert_eq!(2, result[0].count());
    }

    #[test]
    fn find_by_pattern_returns_optional_domain() {
        let adapter = adapter();
        adapter.save(note("n1", "pattern", 2)).unwrap();

        let found = adapter.find_by_pattern("pattern").unwrap();
        assert_eq!("summary", found.unwrap().short_summary());
        assert!(adapter.find_by_pattern("other").unwrap().is_none());
    }

    #[test]
    fn save_maps_domain_to_row_and_back() {
        let adapter = adapter();
        let saved = adapter.save(note("n1", "pattern", 2)).unwrap();

        assert_eq!("n1", saved.id().value());
        assert_eq!("summary", saved.short_summary());
        assert_eq!(ts(), saved.last_seen_at());
    }

    #[test]
    fn save_upserts_by_id_keeps_order_and_allows_duplicate_patterns() {
        let adapter = adapter();
        adapter.save(note("n2", "p", 1)).unwrap();
        adapter.save(note("n1", "p", 1)).unwrap();
        adapter.save(note("n2", "p2", 5)).unwrap();

        let all = adapter.find_all().unwrap();
        let ids: Vec<&str> = all.iter().map(|n| n.id().value()).collect();
        assert_eq!(["n2", "n1"], ids.as_slice());
        assert_eq!(5, all[0].count());
        assert_eq!("p2", all[0].pattern());
        assert_eq!(
            "n1",
            adapter.find_by_pattern("p").unwrap().unwrap().id().value()
        );
    }

    #[test]
    fn delete_by_id_removes_row() {
        let adapter = adapter();
        adapter.save(note("n1", "a", 1)).unwrap();
        adapter.save(note("n2", "b", 1)).unwrap();

        adapter
            .delete_by_id(&WrongNoteId::of("n2").unwrap())
            .unwrap();
        adapter
            .delete_by_id(&WrongNoteId::of("missing").unwrap())
            .unwrap();
        assert_eq!(1, adapter.find_all().unwrap().len());
        adapter.delete_all().unwrap();
        assert!(adapter.find_all().unwrap().is_empty());
    }

    fn queue() -> SqliteWrongNoteRecentQueueAdapter {
        SqliteWrongNoteRecentQueueAdapter::new(Database::open_in_memory().unwrap())
    }

    #[test]
    fn load_global_queue_returns_empty_when_missing() {
        assert_eq!(Vec::<String>::new(), queue().load_global_queue().unwrap());
    }

    #[test]
    fn save_global_queue_clamps_to_100() {
        let queue = queue();
        let input: Vec<String> = (0..140).map(|i| format!("p-{i}")).collect();

        queue.save_global_queue(&input).unwrap();

        let saved = queue.load_global_queue().unwrap();
        assert_eq!(100, saved.len());
        assert_eq!("p-40", saved[0]);
        assert_eq!("p-139", saved[99]);
    }

    #[test]
    fn save_global_queue_replaces_and_clear_removes() {
        let queue = queue();
        queue.save_global_queue(&["a".into(), "b".into()]).unwrap();
        queue.save_global_queue(&["c".into()]).unwrap();
        assert_eq!(vec!["c".to_string()], queue.load_global_queue().unwrap());

        queue.clear_global_queue().unwrap();
        assert!(queue.load_global_queue().unwrap().is_empty());
        queue.clear_global_queue().unwrap();
    }
}
