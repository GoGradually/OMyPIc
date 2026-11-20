//! SQLite 스키마와 버전 관리(`PRAGMA user_version`).

use std::path::{Component, Path};

use rusqlite::Connection;

use super::db_err;
use omypic_domain::error::{AppError, AppResult};

/// 이 빌드가 만드는 스키마 버전.
///
/// - 1: 0.3.0의 첫 스키마.
/// - 2: 룰북 파일 경로를 데이터 폴더 기준 상대 경로로 저장한다. 데이터 폴더 안을 가리키던
///   절대 경로를 상대 경로로 바꾸고(밖을 가리키는 경로는 그대로 둔다), 지운 룰북이 남긴 색인
///   조각을 지운다.
pub const CURRENT_VERSION: i64 = 2;

const SCHEMA_V1: &str = "
CREATE TABLE question_groups (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    tags_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE TABLE question_items (
    id TEXT NOT NULL,
    group_id TEXT NOT NULL REFERENCES question_groups(id) ON DELETE CASCADE,
    position INTEGER NOT NULL,
    text TEXT NOT NULL,
    question_type TEXT,
    PRIMARY KEY (group_id, id)
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

/// `user_version`을 현재 버전까지 올린다. 0이면 스키마를 새로 만든다. 모든 단계는 한
/// 트랜잭션 안에서 하므로 중간에 실패하면 아무것도 바뀌지 않는다. `data_dir`는 데이터베이스가
/// 든 데이터 폴더다(메모리 데이터베이스면 `None`).
///
/// 버전과 상관없이 열 때마다 룰북 행이 없는 색인 조각을 지운다(업로드가 색인을 쓴 뒤 행을
/// 쓰기 전에 프로세스가 죽으면 남는다).
pub fn migrate(conn: &Connection, data_dir: Option<&Path>) -> AppResult<()> {
    upgrade(conn, data_dir)?;
    delete_orphan_chunks(conn)
}

fn upgrade(conn: &Connection, data_dir: Option<&Path>) -> AppResult<()> {
    match read_version(conn)? {
        CURRENT_VERSION => return Ok(()),
        version if !(0..CURRENT_VERSION).contains(&version) => return Err(unsupported(version)),
        _ => {}
    }
    conn.execute_batch("BEGIN IMMEDIATE").map_err(db_err)?;
    // 쓰기 잠금을 얻은 뒤 다시 읽는다. 그 사이 다른 프로세스가 끝냈을 수 있다.
    let result = (|| loop {
        match read_version(conn)? {
            0 => conn.execute_batch(SCHEMA_V1).map_err(db_err)?,
            1 => migrate_to_v2(conn, data_dir)?,
            CURRENT_VERSION => return Ok(()),
            other => return Err(unsupported(other)),
        }
        let next = read_version(conn)? + 1;
        conn.execute_batch(&format!("PRAGMA user_version = {next}"))
            .map_err(db_err)?;
    })();
    match result {
        Ok(()) => conn.execute_batch("COMMIT").map_err(db_err),
        Err(err) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(err)
        }
    }
}

fn migrate_to_v2(conn: &Connection, data_dir: Option<&Path>) -> AppResult<()> {
    if let Some(data_dir) = data_dir {
        let rows: Vec<(String, String)> = conn
            .prepare("SELECT id, path FROM rulebooks")
            .and_then(|mut statement| {
                statement
                    .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
                    .collect()
            })
            .map_err(db_err)?;
        for (id, path) in rows {
            if let Some(relative) = relative_to(Path::new(&path), data_dir) {
                conn.execute(
                    "UPDATE rulebooks SET path = ?1 WHERE id = ?2",
                    rusqlite::params![relative, id],
                )
                .map_err(db_err)?;
            }
        }
    }
    delete_orphan_chunks(conn)
}

fn delete_orphan_chunks(conn: &Connection) -> AppResult<()> {
    let removed = conn
        .execute(
            "DELETE FROM rulebook_chunks WHERE rulebook_id IS NULL
                 OR rulebook_id NOT IN (SELECT id FROM rulebooks)",
            [],
        )
        .map_err(db_err)?;
    if removed > 0 {
        tracing::info!("Removed {removed} rulebook index chunks without a rulebook");
    }
    Ok(())
}

/// `path`가 `data_dir` 안을 가리키면 `/`로 이은 상대 경로. 0.3.0은 데이터 폴더 설정을 그대로
/// 이어 붙여 저장했으므로 데이터 폴더가 상대 경로로 설정됐으면 저장된 경로도 그 꼴이다.
fn relative_to(path: &Path, data_dir: &Path) -> Option<String> {
    if data_dir.as_os_str().is_empty() {
        return None;
    }
    let rest = path.strip_prefix(data_dir).ok()?;
    let parts: Option<Vec<&str>> = rest
        .components()
        .map(|part| match part {
            Component::Normal(name) => name.to_str(),
            _ => None,
        })
        .collect();
    let parts = parts.filter(|parts| !parts.is_empty())?;
    Some(parts.join("/"))
}

fn read_version(conn: &Connection) -> AppResult<i64> {
    conn.query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(db_err)
}

fn unsupported(version: i64) -> AppError {
    if version > CURRENT_VERSION {
        return AppError::InvalidState(format!(
            "Unsupported database version: {version}. A newer OMyPIc created this database; \
             this version reads up to version {CURRENT_VERSION}. Update OMyPIc to open it."
        ));
    }
    AppError::InvalidState(format!("Unsupported database version: {version}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};
    use std::time::Duration;

    /// 0.3.0이 만든 버전 1 데이터베이스.
    fn version_1_database(path: &std::path::Path) -> Connection {
        let conn = Connection::open(path).unwrap();
        conn.execute_batch(SCHEMA_V1).unwrap();
        conn.execute_batch("PRAGMA user_version = 1").unwrap();
        conn
    }

    fn insert_rulebook(conn: &Connection, id: &str, path: &str) {
        conn.execute(
            "INSERT INTO rulebooks (id, filename, path, scope, question_group, enabled, created_at, updated_at)
             VALUES (?1, 'rules.md', ?2, 'MAIN', NULL, 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            rusqlite::params![id, path],
        )
        .unwrap();
    }

    fn insert_chunk(conn: &Connection, rulebook_id: &str) {
        conn.execute(
            "INSERT INTO rulebook_chunks (rulebook_id, filename, position, text, embedding)
             VALUES (?1, 'rules.md', 0, 'text', x'00')",
            rusqlite::params![rulebook_id],
        )
        .unwrap();
    }

    #[test]
    fn version_2_stores_rulebook_paths_inside_the_data_folder_relative_to_it() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        let db_path = data.join("omypic.db");
        let inside = data.join("rulebooks").join("1700000000000_rules.md");
        let outside = dir.path().join("elsewhere").join("notes.md");
        {
            let conn = version_1_database(&db_path);
            insert_rulebook(&conn, "inside", &inside.to_string_lossy());
            insert_rulebook(&conn, "outside", &outside.to_string_lossy());
            insert_rulebook(&conn, "relative", "rulebooks/already.md");
            insert_chunk(&conn, "inside");
            insert_chunk(&conn, "deleted-long-ago");
        }

        let db = crate::sqlite::Database::open(&db_path).unwrap();

        db.with_conn(|conn| {
            assert_eq!(CURRENT_VERSION, read_version(conn)?);
            let path_of = |id: &str| -> String {
                conn.query_row("SELECT path FROM rulebooks WHERE id = ?1", [id], |row| {
                    row.get(0)
                })
                .unwrap()
            };
            assert_eq!("rulebooks/1700000000000_rules.md", path_of("inside"));
            assert_eq!(outside.to_string_lossy(), path_of("outside"));
            assert_eq!("rulebooks/already.md", path_of("relative"));
            let chunk_owners: Vec<String> = conn
                .prepare("SELECT rulebook_id FROM rulebook_chunks")
                .unwrap()
                .query_map([], |row| row.get(0))
                .unwrap()
                .map(Result::unwrap)
                .collect();
            assert_eq!(vec!["inside".to_string()], chunk_owners);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn new_database_starts_at_the_current_version() {
        let dir = tempfile::tempdir().unwrap();
        let db = crate::sqlite::Database::open(&dir.path().join("omypic.db")).unwrap();
        db.with_conn(|conn| {
            assert_eq!(2, read_version(conn)?);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn a_database_from_a_newer_omypic_is_refused_with_a_message_that_says_so() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA user_version = 3").unwrap();

        let err = migrate(&conn, None).unwrap_err();

        assert_eq!(
            AppError::InvalidState(
                "Unsupported database version: 3. A newer OMyPIc created this database; \
                 this version reads up to version 2. Update OMyPIc to open it."
                    .to_string()
            ),
            err
        );
        let negative = Connection::open_in_memory().unwrap();
        negative.execute_batch("PRAGMA user_version = -1").unwrap();
        assert_eq!(
            AppError::InvalidState("Unsupported database version: -1".to_string()),
            migrate(&negative, None).unwrap_err()
        );
    }

    #[test]
    fn orphan_index_chunks_are_removed_on_every_open() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("omypic.db");
        drop(crate::sqlite::Database::open(&db_path).unwrap());
        {
            // Left behind at the current version, e.g. by a crash between the index write and
            // the row write of an upload.
            let conn = Connection::open(&db_path).unwrap();
            insert_rulebook(&conn, "kept", "rulebooks/kept.md");
            insert_chunk(&conn, "kept");
            insert_chunk(&conn, "orphan");
            conn.execute(
                "INSERT INTO rulebook_chunks (rulebook_id, filename, position, text, embedding)
                 VALUES (NULL, 'x.md', 0, 'text', x'00')",
                [],
            )
            .unwrap();
        }

        let db = crate::sqlite::Database::open(&db_path).unwrap();

        db.with_conn(|conn| {
            let owners: Vec<Option<String>> = conn
                .prepare("SELECT rulebook_id FROM rulebook_chunks")
                .unwrap()
                .query_map([], |row| row.get(0))
                .unwrap()
                .map(Result::unwrap)
                .collect();
            assert_eq!(vec![Some("kept".to_string())], owners);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn concurrent_openers_migrate_once_without_error() {
        for round in 0..10 {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join(format!("omypic-{round}.db"));
            let barrier = Arc::new(Barrier::new(6));
            let handles: Vec<_> = (0..6)
                .map(|_| {
                    let path = path.clone();
                    let barrier = barrier.clone();
                    std::thread::spawn(move || {
                        let conn = Connection::open(&path).unwrap();
                        conn.busy_timeout(Duration::from_secs(5)).unwrap();
                        barrier.wait();
                        migrate(&conn, None)
                    })
                })
                .collect();
            for handle in handles {
                handle.join().unwrap().unwrap();
            }
            let conn = Connection::open(&path).unwrap();
            assert_eq!(CURRENT_VERSION, read_version(&conn).unwrap());
        }
    }
}
