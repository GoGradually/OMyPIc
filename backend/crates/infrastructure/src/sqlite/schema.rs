//! SQLite 스키마와 버전 관리(`PRAGMA user_version`).

use rusqlite::Connection;

use super::db_err;
use omypic_domain::error::{AppError, AppResult};

/// 이 빌드가 만드는 스키마 버전.
pub const CURRENT_VERSION: i64 = 1;

const SCHEMA_V1: &str = "
CREATE TABLE question_groups (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    tags_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE TABLE question_items (
    id TEXT PRIMARY KEY,
    group_id TEXT NOT NULL REFERENCES question_groups(id) ON DELETE CASCADE,
    position INTEGER NOT NULL,
    text TEXT NOT NULL,
    question_type TEXT
);
CREATE INDEX idx_question_items_group ON question_items(group_id, position);
CREATE TABLE rulebooks (
    id TEXT PRIMARY KEY,
    filename TEXT NOT NULL,
    path TEXT NOT NULL,
    scope TEXT NOT NULL,
    question_group TEXT,
    enabled INTEGER NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE TABLE wrong_notes (
    id TEXT PRIMARY KEY,
    pattern TEXT NOT NULL,
    count INTEGER NOT NULL,
    short_summary TEXT NOT NULL,
    last_seen_at TEXT NOT NULL
);
CREATE INDEX idx_wrong_notes_pattern ON wrong_notes(pattern);
CREATE TABLE wrong_note_queue_states (
    id TEXT PRIMARY KEY,
    patterns_json TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE TABLE rulebook_chunks (
    id INTEGER PRIMARY KEY,
    rulebook_id TEXT,
    filename TEXT,
    position INTEGER,
    text TEXT,
    embedding BLOB
);
CREATE TABLE rulebook_index_meta (
    key TEXT PRIMARY KEY,
    value TEXT
);
";

/// `user_version`이 0이면 버전 1 스키마를 만들고, 1이면 아무것도 하지 않는다.
pub fn migrate(conn: &Connection) -> AppResult<()> {
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(db_err)?;
    match version {
        0 => {
            conn.execute_batch("BEGIN IMMEDIATE").map_err(db_err)?;
            let result = conn.execute_batch(SCHEMA_V1).and_then(|()| {
                conn.execute_batch(&format!("PRAGMA user_version = {CURRENT_VERSION}"))
            });
            match result {
                Ok(()) => conn.execute_batch("COMMIT").map_err(db_err),
                Err(err) => {
                    let _ = conn.execute_batch("ROLLBACK");
                    Err(db_err(err))
                }
            }
        }
        CURRENT_VERSION => Ok(()),
        other => Err(AppError::InvalidState(format!(
            "Unsupported database version: {other}"
        ))),
    }
}
