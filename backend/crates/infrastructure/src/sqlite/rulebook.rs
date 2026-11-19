use rusqlite::{params, Connection, OptionalExtension};

use omypic_application::rulebook::port::RulebookPort;
use omypic_domain::error::{AppError, AppResult};
use omypic_domain::question::QuestionGroup;
use omypic_domain::rulebook::{Rulebook, RulebookId, RulebookScope};

use super::{db_err, format_ts, parse_ts, Database};

pub struct SqliteRulebookAdapter {
    db: Database,
}

impl SqliteRulebookAdapter {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

const COLUMNS: &str = "id, filename, path, scope, question_group, enabled, created_at, updated_at";

struct Row {
    id: String,
    filename: String,
    path: String,
    scope: String,
    question_group: Option<String>,
    enabled: bool,
    created_at: String,
    updated_at: String,
}

fn read_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Row> {
    Ok(Row {
        id: row.get(0)?,
        filename: row.get(1)?,
        path: row.get(2)?,
        scope: row.get(3)?,
        question_group: row.get(4)?,
        enabled: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

fn scope_name(scope: RulebookScope) -> &'static str {
    match scope {
        RulebookScope::Main => "MAIN",
        RulebookScope::Question => "QUESTION",
    }
}

fn to_domain(row: Row) -> AppResult<Rulebook> {
    let scope = match row.scope.as_str() {
        "MAIN" => RulebookScope::Main,
        "QUESTION" => RulebookScope::Question,
        other => {
            return Err(AppError::Internal(format!(
                "Unknown rulebook scope: {other}"
            )))
        }
    };
    Rulebook::rehydrate(
        RulebookId::of(&row.id)?,
        &row.filename,
        &row.path,
        Some(scope),
        QuestionGroup::from_nullable(row.question_group.as_deref()),
        row.enabled,
        Some(parse_ts(&row.created_at)?),
        Some(parse_ts(&row.updated_at)?),
    )
}

fn find_by_id(conn: &Connection, id: &str) -> AppResult<Option<Rulebook>> {
    conn.query_row(
        &format!("SELECT {COLUMNS} FROM rulebooks WHERE id = ?1"),
        params![id],
        read_row,
    )
    .optional()
    .map_err(db_err)?
    .map(to_domain)
    .transpose()
}

/// 룰북 행 하나를 쓴다. ON CONFLICT 갱신은 rowid를 유지해 목록 순서를 바꾸지 않는다.
pub(super) fn upsert_rulebook(conn: &Connection, rulebook: &Rulebook) -> AppResult<()> {
    conn.execute(
        &format!(
            "INSERT INTO rulebooks ({COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(id) DO UPDATE SET filename = excluded.filename,
                 path = excluded.path, scope = excluded.scope,
                 question_group = excluded.question_group, enabled = excluded.enabled,
                 created_at = excluded.created_at, updated_at = excluded.updated_at"
        ),
        params![
            rulebook.id().value(),
            rulebook.filename(),
            rulebook.path(),
            scope_name(rulebook.scope()),
            rulebook.question_group().map(QuestionGroup::value),
            rulebook.is_enabled(),
            format_ts(rulebook.created_at()),
            format_ts(rulebook.updated_at()),
        ],
    )
    .map(|_| ())
    .map_err(db_err)
}

impl RulebookPort for SqliteRulebookAdapter {
    fn find_all(&self) -> AppResult<Vec<Rulebook>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn
                .prepare(&format!("SELECT {COLUMNS} FROM rulebooks ORDER BY rowid"))
                .map_err(db_err)?;
            let rows = stmt
                .query_map([], read_row)
                .map_err(db_err)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(db_err)?;
            rows.into_iter().map(to_domain).collect()
        })
    }

    fn find_by_id(&self, id: &RulebookId) -> AppResult<Option<Rulebook>> {
        self.db.with_conn(|conn| find_by_id(conn, id.value()))
    }

    fn save(&self, rulebook: Rulebook) -> AppResult<Rulebook> {
        self.db.with_conn(|conn| {
            upsert_rulebook(conn, &rulebook)?;
            find_by_id(conn, rulebook.id().value())?
                .ok_or_else(|| AppError::Internal("Saved rulebook not found".to_string()))
        })
    }

    fn delete_by_id(&self, id: &RulebookId) -> AppResult<()> {
        self.db.with_conn(|conn| {
            conn.execute("DELETE FROM rulebooks WHERE id = ?1", params![id.value()])
                .map(|_| ())
                .map_err(db_err)
        })
    }

    fn delete_all(&self) -> AppResult<()> {
        self.db.with_conn(|conn| {
            conn.execute("DELETE FROM rulebooks", [])
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

    fn adapter() -> SqliteRulebookAdapter {
        SqliteRulebookAdapter::new(Database::open_in_memory().unwrap())
    }

    fn rulebook(id: &str, filename: &str, scope: RulebookScope, group: Option<&str>) -> Rulebook {
        Rulebook::rehydrate(
            RulebookId::of(id).unwrap(),
            filename,
            &format!("/tmp/{filename}"),
            Some(scope),
            group.map(|g| QuestionGroup::of(g).unwrap()),
            true,
            Some(ts()),
            Some(ts()),
        )
        .unwrap()
    }

    #[test]
    fn find_all_maps_rows_to_domain() {
        let adapter = adapter();
        adapter
            .save(rulebook(
                "r1",
                "rules.md",
                RulebookScope::Question,
                Some("A"),
            ))
            .unwrap();

        let result = adapter.find_all().unwrap();

        assert_eq!(1, result.len());
        assert_eq!("r1", result[0].id().value());
        assert_eq!("rules.md", result[0].filename());
        assert_eq!(RulebookScope::Question, result[0].scope());
        assert_eq!(
            Some(&QuestionGroup::of("A").unwrap()),
            result[0].question_group()
        );
        assert!(result[0].is_enabled());
    }

    #[test]
    fn find_by_id_returns_optional_domain() {
        let adapter = adapter();
        adapter
            .save(rulebook(
                "r1",
                "rules.md",
                RulebookScope::Question,
                Some("A"),
            ))
            .unwrap();

        let found = adapter.find_by_id(&RulebookId::of("r1").unwrap()).unwrap();
        assert_eq!("/tmp/rules.md", found.unwrap().path());
        assert!(adapter
            .find_by_id(&RulebookId::of("missing").unwrap())
            .unwrap()
            .is_none());
    }

    #[test]
    fn save_maps_domain_to_row_and_back() {
        let adapter = adapter();
        let saved = adapter
            .save(rulebook(
                "r1",
                "rules.md",
                RulebookScope::Question,
                Some("A"),
            ))
            .unwrap();

        assert_eq!("r1", saved.id().value());
        assert_eq!("rules.md", saved.filename());
        assert_eq!("/tmp/rules.md", saved.path());
        assert_eq!(RulebookScope::Question, saved.scope());
        assert_eq!("A", saved.question_group().unwrap().value());
        assert!(saved.is_enabled());
        assert_eq!(ts(), saved.created_at());
        assert_eq!(ts(), saved.updated_at());
    }

    #[test]
    fn save_updates_existing_row_in_place_and_keeps_order() {
        let adapter = adapter();
        adapter
            .save(rulebook("r2", "b.md", RulebookScope::Main, None))
            .unwrap();
        adapter
            .save(rulebook("r1", "a.md", RulebookScope::Main, None))
            .unwrap();
        let mut changed = rulebook("r2", "b.md", RulebookScope::Main, None);
        changed.toggle(false, ts());
        adapter.save(changed).unwrap();

        let all = adapter.find_all().unwrap();
        let ids: Vec<&str> = all.iter().map(|r| r.id().value()).collect();
        assert_eq!(["r2", "r1"], ids.as_slice());
        assert!(!all[0].is_enabled());
        assert!(all[0].question_group().is_none());
    }

    #[test]
    fn delete_by_id_removes_row() {
        let adapter = adapter();
        adapter
            .save(rulebook("r1", "a.md", RulebookScope::Main, None))
            .unwrap();
        adapter
            .save(rulebook("r2", "b.md", RulebookScope::Main, None))
            .unwrap();

        adapter
            .delete_by_id(&RulebookId::of("r2").unwrap())
            .unwrap();
        adapter
            .delete_by_id(&RulebookId::of("missing").unwrap())
            .unwrap();

        let all = adapter.find_all().unwrap();
        assert_eq!(1, all.len());
        assert_eq!("r1", all[0].id().value());
        adapter.delete_all().unwrap();
        assert!(adapter.find_all().unwrap().is_empty());
    }

    #[test]
    fn rulebook_keeps_korean_filename() {
        let adapter = adapter();
        adapter
            .save(rulebook("r1", "오픽 규칙.md", RulebookScope::Main, None))
            .unwrap();

        let found = adapter
            .find_by_id(&RulebookId::of("r1").unwrap())
            .unwrap()
            .unwrap();
        assert_eq!("오픽 규칙.md", found.filename());
        assert_eq!("/tmp/오픽 규칙.md", found.path());
    }
}
