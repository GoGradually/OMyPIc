use std::collections::{HashMap, HashSet};

use rusqlite::{params, Connection};

use omypic_application::question::port::QuestionGroupPort;
use omypic_domain::error::{AppError, AppResult};
use omypic_domain::question::{
    QuestionGroupAggregate, QuestionGroupId, QuestionItem, QuestionItemId,
};

use super::{db_err, format_ts, parse_ts, Database};

pub struct SqliteQuestionGroupAdapter {
    db: Database,
}

impl SqliteQuestionGroupAdapter {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

struct GroupRow {
    id: String,
    name: String,
    tags_json: String,
    created_at: String,
    updated_at: String,
}

struct ItemRow {
    id: String,
    text: String,
    question_type: Option<String>,
}

/// 저장된 순서(rowid)대로 그룹을 읽는다. Mongo의 `findAll`도 정렬 없이 저장 순서였다.
fn load_groups(
    conn: &Connection,
    only: Option<&HashSet<&str>>,
) -> AppResult<Vec<QuestionGroupAggregate>> {
    let mut stmt = conn
        .prepare("SELECT id, name, tags_json, created_at, updated_at FROM question_groups ORDER BY rowid")
        .map_err(db_err)?;
    let rows = stmt
        .query_map([], |row| {
            Ok(GroupRow {
                id: row.get(0)?,
                name: row.get(1)?,
                tags_json: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
            })
        })
        .map_err(db_err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_err)?;

    let mut items: HashMap<String, Vec<ItemRow>> = HashMap::new();
    let mut stmt = conn
        .prepare("SELECT group_id, id, text, question_type FROM question_items ORDER BY group_id, position")
        .map_err(db_err)?;
    let item_rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                ItemRow {
                    id: row.get(1)?,
                    text: row.get(2)?,
                    question_type: row.get(3)?,
                },
            ))
        })
        .map_err(db_err)?;
    for item in item_rows {
        let (group_id, item) = item.map_err(db_err)?;
        items.entry(group_id).or_default().push(item);
    }

    rows.into_iter()
        .filter(|row| only.is_none_or(|ids| ids.contains(row.id.as_str())))
        .map(|row| {
            let questions = items
                .remove(&row.id)
                .unwrap_or_default()
                .into_iter()
                .map(|item| {
                    Ok(QuestionItem::rehydrate(
                        QuestionItemId::of(&item.id)?,
                        &item.text,
                        item.question_type.as_deref(),
                    ))
                })
                .collect::<AppResult<Vec<_>>>()?;
            let tags: Vec<String> = serde_json::from_str(&row.tags_json)
                .map_err(|err| AppError::Internal(format!("Invalid stored tags: {err}")))?;
            QuestionGroupAggregate::rehydrate(
                QuestionGroupId::of(&row.id)?,
                &row.name,
                tags,
                questions,
                Some(parse_ts(&row.created_at)?),
                Some(parse_ts(&row.updated_at)?),
            )
        })
        .collect()
}

/// Java `Stream.sorted()`와 같은 UTF-16 코드 유닛 순서.
fn sorted_tags(group: &QuestionGroupAggregate) -> Vec<String> {
    let mut tags = group.tags().to_vec();
    tags.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
    tags
}

fn write_group(conn: &mut Connection, group: &QuestionGroupAggregate) -> AppResult<()> {
    let tx = conn.transaction().map_err(db_err)?;
    insert_group(&tx, group)?;
    tx.commit().map_err(db_err)
}

/// 그룹 행과 질문 행을 쓴다. 트랜잭션은 부르는 쪽이 연다.
pub(super) fn insert_group(conn: &Connection, group: &QuestionGroupAggregate) -> AppResult<()> {
    let tags_json = serde_json::to_string(&sorted_tags(group))
        .map_err(|err| AppError::Internal(err.to_string()))?;
    // ON CONFLICT 갱신은 rowid를 유지하므로 목록 순서가 바뀌지 않는다(REPLACE는 바꾼다).
    conn.execute(
        "INSERT INTO question_groups (id, name, tags_json, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(id) DO UPDATE SET name = excluded.name, tags_json = excluded.tags_json,
             created_at = excluded.created_at, updated_at = excluded.updated_at",
        params![
            group.id().value(),
            group.name(),
            tags_json,
            format_ts(group.created_at()),
            format_ts(group.updated_at()),
        ],
    )
    .map_err(db_err)?;
    conn.execute(
        "DELETE FROM question_items WHERE group_id = ?1",
        params![group.id().value()],
    )
    .map_err(db_err)?;
    for (position, item) in group.questions().iter().enumerate() {
        conn.execute(
            "INSERT INTO question_items (id, group_id, position, text, question_type)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                item.id().value(),
                group.id().value(),
                position as i64,
                item.text(),
                item.question_type(),
            ],
        )
        .map_err(db_err)?;
    }
    Ok(())
}

impl QuestionGroupPort for SqliteQuestionGroupAdapter {
    fn find_all(&self) -> AppResult<Vec<QuestionGroupAggregate>> {
        self.db.with_conn(|conn| load_groups(conn, None))
    }

    fn find_all_by_id(&self, ids: &[QuestionGroupId]) -> AppResult<Vec<QuestionGroupAggregate>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let wanted: HashSet<&str> = ids.iter().map(QuestionGroupId::value).collect();
        self.db.with_conn(|conn| load_groups(conn, Some(&wanted)))
    }

    fn find_by_id(&self, id: &QuestionGroupId) -> AppResult<Option<QuestionGroupAggregate>> {
        let wanted = HashSet::from([id.value()]);
        self.db
            .with_conn(|conn| Ok(load_groups(conn, Some(&wanted))?.into_iter().next()))
    }

    fn save(&self, group: QuestionGroupAggregate) -> AppResult<QuestionGroupAggregate> {
        let id = group.id().clone();
        self.db.with_conn(|conn| {
            write_group(conn, &group)?;
            let wanted = HashSet::from([id.value()]);
            load_groups(conn, Some(&wanted))?
                .into_iter()
                .next()
                .ok_or_else(|| AppError::Internal("Saved question group not found".to_string()))
        })
    }

    fn delete_by_id(&self, id: &QuestionGroupId) -> AppResult<()> {
        self.db.with_conn(|conn| {
            let tx = conn.transaction().map_err(db_err)?;
            tx.execute(
                "DELETE FROM question_items WHERE group_id = ?1",
                params![id.value()],
            )
            .map_err(db_err)?;
            tx.execute(
                "DELETE FROM question_groups WHERE id = ?1",
                params![id.value()],
            )
            .map_err(db_err)?;
            tx.commit().map_err(db_err)
        })
    }

    fn delete_all(&self) -> AppResult<()> {
        self.db.with_conn(|conn| {
            let tx = conn.transaction().map_err(db_err)?;
            tx.execute("DELETE FROM question_items", [])
                .map_err(db_err)?;
            tx.execute("DELETE FROM question_groups", [])
                .map_err(db_err)?;
            tx.commit().map_err(db_err)
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

    fn adapter() -> SqliteQuestionGroupAdapter {
        SqliteQuestionGroupAdapter::new(Database::open_in_memory().unwrap())
    }

    fn group(
        id: &str,
        name: &str,
        tags: &[&str],
        items: Vec<QuestionItem>,
    ) -> QuestionGroupAggregate {
        QuestionGroupAggregate::rehydrate(
            QuestionGroupId::of(id).unwrap(),
            name,
            tags.iter().copied(),
            items,
            Some(ts()),
            Some(ts()),
        )
        .unwrap()
    }

    fn item(id: &str, text: &str, question_type: Option<&str>) -> QuestionItem {
        QuestionItem::rehydrate(QuestionItemId::of(id).unwrap(), text, question_type)
    }

    #[test]
    fn find_all_maps_rows_to_domain() {
        let adapter = adapter();
        adapter
            .save(group(
                "g1",
                "Travel",
                &["travel", "habit"],
                vec![item("q1", "Question", Some("habit"))],
            ))
            .unwrap();

        let result = adapter.find_all().unwrap();

        assert_eq!(1, result.len());
        assert_eq!("g1", result[0].id().value());
        assert_eq!("Travel", result[0].name());
        assert_eq!("q1", result[0].questions()[0].id().value());
    }

    #[test]
    fn find_by_id_returns_empty_when_missing() {
        let result = adapter()
            .find_by_id(&QuestionGroupId::of("missing").unwrap())
            .unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn save_maps_domain_to_row_and_back() {
        let adapter = adapter();
        let saved = adapter
            .save(group(
                "g1",
                "Travel",
                &["travel", "habit"],
                vec![item("q1", "Question", Some("habit"))],
            ))
            .unwrap();

        assert_eq!("g1", saved.id().value());
        assert_eq!("Travel", saved.name());
        assert_eq!("Question", saved.questions()[0].text());
        assert_eq!(Some("habit"), saved.questions()[0].question_type());
        assert_eq!(ts(), saved.created_at());
        assert_eq!(ts(), saved.updated_at());
        let reread = adapter
            .find_by_id(&QuestionGroupId::of("g1").unwrap())
            .unwrap()
            .unwrap();
        assert_eq!("Question", reread.questions()[0].text());
    }

    #[test]
    fn save_stores_tags_sorted_like_the_mongo_adapter() {
        let adapter = adapter();
        let saved = adapter
            .save(group("g1", "Travel", &["travel", "habit", "a"], vec![]))
            .unwrap();
        assert_eq!(["a", "habit", "travel"], saved.tags());
    }

    #[test]
    fn delete_by_id_removes_group_and_its_items() {
        let adapter = adapter();
        adapter
            .save(group("g1", "A", &[], vec![item("q1", "t", None)]))
            .unwrap();
        adapter.save(group("g2", "B", &[], vec![])).unwrap();

        adapter
            .delete_by_id(&QuestionGroupId::of("g1").unwrap())
            .unwrap();
        adapter
            .delete_by_id(&QuestionGroupId::of("missing").unwrap())
            .unwrap();

        let remaining = adapter.find_all().unwrap();
        assert_eq!(1, remaining.len());
        assert_eq!("g2", remaining[0].id().value());
        let items: i64 = adapter
            .db
            .with_conn(|c| {
                c.query_row("SELECT COUNT(*) FROM question_items", [], |r| r.get(0))
                    .map_err(db_err)
            })
            .unwrap();
        assert_eq!(0, items);
    }

    #[test]
    fn delete_all_clears_groups_and_items() {
        let adapter = adapter();
        adapter
            .save(group("g1", "A", &[], vec![item("q1", "t", None)]))
            .unwrap();
        adapter.delete_all().unwrap();
        assert!(adapter.find_all().unwrap().is_empty());
    }

    #[test]
    fn find_all_keeps_insertion_order_and_save_updates_in_place() {
        let adapter = adapter();
        for id in ["z", "a", "m"] {
            adapter.save(group(id, id, &[], vec![])).unwrap();
        }
        adapter.save(group("z", "renamed", &[], vec![])).unwrap();

        let all = adapter.find_all().unwrap();
        let ids: Vec<&str> = all.iter().map(|g| g.id().value()).collect();
        assert_eq!(["z", "a", "m"], ids.as_slice());
        assert_eq!("renamed", all[0].name());

        let wanted = [
            QuestionGroupId::of("m").unwrap(),
            QuestionGroupId::of("z").unwrap(),
        ];
        let found = adapter.find_all_by_id(&wanted).unwrap();
        let ids: Vec<&str> = found.iter().map(|g| g.id().value()).collect();
        assert_eq!(["z", "m"], ids.as_slice());
        assert!(adapter.find_all_by_id(&[]).unwrap().is_empty());
    }

    #[test]
    fn question_group_keeps_item_order() {
        let adapter = adapter();
        // id 순서와 저장 순서가 다르게 한다(id로 정렬하면 실패한다).
        for id in ["g5", "g4", "g3", "g2", "g1"] {
            adapter
                .save(group(
                    id,
                    id,
                    &[],
                    vec![
                        item(&format!("{id}-c"), "first", None),
                        item(&format!("{id}-a"), "second", Some("habit")),
                        item(&format!("{id}-b"), "third", None),
                    ],
                ))
                .unwrap();
        }

        let all = adapter.find_all().unwrap();
        let group_ids: Vec<&str> = all.iter().map(|g| g.id().value()).collect();
        assert_eq!(["g5", "g4", "g3", "g2", "g1"], group_ids.as_slice());
        let item_ids: Vec<&str> = all[0].questions().iter().map(|q| q.id().value()).collect();
        assert_eq!(["g5-c", "g5-a", "g5-b"], item_ids.as_slice());
    }

    #[test]
    fn same_item_id_in_two_groups_is_saved_and_read_back() {
        let adapter = adapter();
        adapter
            .save(group("g1", "A", &[], vec![item("q1", "from a", None)]))
            .unwrap();
        adapter
            .save(group(
                "g2",
                "B",
                &[],
                vec![item("q1", "from b", Some("habit"))],
            ))
            .unwrap();

        let all = adapter.find_all().unwrap();
        assert_eq!(2, all.len());
        assert_eq!("from a", all[0].questions()[0].text());
        assert_eq!("from b", all[1].questions()[0].text());
        assert_eq!("q1", all[1].questions()[0].id().value());
    }
}
