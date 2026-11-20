//! 룰북 청크 인덱스(Lucene 대체). 청크와 임베딩을 `rulebook_chunks`에 저장하고,
//! 검색은 활성 룰북의 청크 전부와 질의 벡터의 유클리드 거리를 계산해 가까운 순으로 고른다.

use std::collections::HashSet;
use std::sync::Arc;

use rusqlite::{params, params_from_iter, Connection, OptionalExtension, Transaction};

use omypic_application::rulebook::port::{EmbeddingPort, RulebookIndexPort};
use omypic_domain::error::{AppError, AppResult};
use omypic_domain::rulebook::{RulebookContext, RulebookId};

use super::{db_err, write_transaction, Database};

const META_PROVIDER: &str = "provider";
const META_MODEL_VERSION: &str = "modelVersion";
const META_DIMENSION: &str = "dimension";
/// SQLite의 바인딩 변수 한도(기본 32766)보다 한참 작게 나눈다.
const ID_BATCH: usize = 500;

pub struct SqliteRulebookIndexAdapter {
    db: Database,
    embedding: Arc<dyn EmbeddingPort>,
}

impl SqliteRulebookIndexAdapter {
    pub fn new(db: Database, embedding: Arc<dyn EmbeddingPort>) -> Self {
        Self { db, embedding }
    }

    fn expected_metadata(&self) -> [(&'static str, String); 3] {
        index_metadata(self.embedding.as_ref())
    }

    /// 저장된 메타데이터가 현재 임베딩과 다르면 청크를 모두 지우고 메타데이터를 갱신한다.
    fn ensure_index_compatibility(&self, tx: &Transaction<'_>) -> AppResult<()> {
        let expected = self.expected_metadata();
        let mut compatible = true;
        for (key, value) in &expected {
            let stored: Option<String> = tx
                .query_row(
                    "SELECT value FROM rulebook_index_meta WHERE key = ?1",
                    [key],
                    |row| row.get(0),
                )
                .optional()
                .map_err(db_err)?;
            compatible &= stored.as_deref() == Some(value.as_str());
        }
        if !compatible {
            tx.execute("DELETE FROM rulebook_chunks", [])
                .map_err(db_err)?;
            write_metadata(tx, &expected)?;
        }
        Ok(())
    }
}

pub(super) fn index_metadata(embedding: &dyn EmbeddingPort) -> [(&'static str, String); 3] {
    [
        (META_PROVIDER, embedding.provider()),
        (META_MODEL_VERSION, embedding.model_version()),
        (META_DIMENSION, embedding.dimension().to_string()),
    ]
}

/// 조각마다 임베딩을 계산해 BLOB으로 만든다. 연결 잠금 밖에서 부른다.
pub(super) fn embed_chunks(
    embedding: &dyn EmbeddingPort,
    chunks: &[String],
) -> AppResult<Vec<Vec<u8>>> {
    let dimension = embedding.dimension();
    let mut rows = Vec::with_capacity(chunks.len());
    for chunk in chunks {
        let vector = embedding.embed(chunk)?;
        if vector.len() != dimension {
            return Err(AppError::InvalidState(format!(
                "Embedding dimension mismatch. expected={dimension} actual={}",
                vector.len()
            )));
        }
        rows.push(encode_embedding(&vector));
    }
    Ok(rows)
}

pub(super) fn insert_chunks(
    conn: &Connection,
    rulebook_id: &RulebookId,
    filename: &str,
    chunks: &[String],
    embeddings: &[Vec<u8>],
) -> AppResult<()> {
    let mut insert = conn
        .prepare_cached(
            "INSERT INTO rulebook_chunks (rulebook_id, filename, position, text, embedding)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .map_err(db_err)?;
    for (position, (chunk, blob)) in chunks.iter().zip(embeddings).enumerate() {
        insert
            .execute(params![
                rulebook_id.value(),
                filename,
                position as i64,
                chunk,
                blob
            ])
            .map_err(db_err)?;
    }
    Ok(())
}

pub(super) fn write_metadata(
    conn: &Connection,
    metadata: &[(&'static str, String)],
) -> AppResult<()> {
    for (key, value) in metadata {
        conn.execute(
            "INSERT INTO rulebook_index_meta (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )
        .map_err(db_err)?;
    }
    Ok(())
}

fn encode_embedding(vector: &[f32]) -> Vec<u8> {
    vector.iter().flat_map(|v| v.to_le_bytes()).collect()
}

fn decode_embedding(blob: &[u8]) -> Vec<f32> {
    blob.chunks_exact(4)
        .map(|bytes| f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
        .collect()
}

/// 제곱 유클리드 거리. 순서만 필요해서 제곱근은 구하지 않는다.
fn squared_distance(a: &[f32], b: &[f32]) -> f32 {
    let mut sum = 0.0_f32;
    for (x, y) in a.iter().zip(b) {
        let diff = x - y;
        sum += diff * diff;
    }
    sum
}

impl RulebookIndexPort for SqliteRulebookIndexAdapter {
    fn index_rulebook_chunks(
        &self,
        rulebook_id: &RulebookId,
        filename: &str,
        chunks: &[String],
    ) -> AppResult<()> {
        // 임베딩 계산은 연결 잠금 밖에서 끝낸다.
        let rows = embed_chunks(self.embedding.as_ref(), chunks)?;
        self.db.with_conn(|conn| {
            let tx = write_transaction(conn)?;
            self.ensure_index_compatibility(&tx)?;
            insert_chunks(&tx, rulebook_id, filename, chunks, &rows)?;
            tx.commit().map_err(db_err)
        })
    }

    fn search(
        &self,
        query: &str,
        top_k: usize,
        enabled_rulebook_ids: &HashSet<RulebookId>,
    ) -> AppResult<Vec<RulebookContext>> {
        if enabled_rulebook_ids.is_empty() || top_k == 0 {
            return Ok(Vec::new());
        }
        let dimension = self.embedding.dimension();
        let query_vector = self.embedding.embed(query)?;
        if query_vector.len() != dimension {
            return Err(AppError::InvalidState(format!(
                "Embedding dimension mismatch. expected={dimension} actual={}",
                query_vector.len()
            )));
        }
        let ids: Vec<&str> = enabled_rulebook_ids.iter().map(RulebookId::value).collect();

        // 1단계: 호환성을 맞추고 임베딩만 읽는다.
        let candidates = self.db.with_conn(|conn| {
            let tx = write_transaction(conn)?;
            self.ensure_index_compatibility(&tx)?;
            let mut found: Vec<(i64, Vec<u8>)> = Vec::new();
            for batch in ids.chunks(ID_BATCH) {
                let placeholders = vec!["?"; batch.len()].join(",");
                let sql = format!(
                    "SELECT id, embedding FROM rulebook_chunks WHERE rulebook_id IN ({placeholders})"
                );
                let mut statement = tx.prepare(&sql).map_err(db_err)?;
                let rows = statement
                    .query_map(params_from_iter(batch.iter()), |row| {
                        Ok((row.get::<_, i64>(0)?, row.get::<_, Option<Vec<u8>>>(1)?))
                    })
                    .map_err(db_err)?;
                for row in rows {
                    let (id, blob) = row.map_err(db_err)?;
                    found.push((id, blob.unwrap_or_default()));
                }
            }
            tx.commit().map_err(db_err)?;
            Ok(found)
        })?;

        // 2단계: 거리 계산과 정렬은 잠금 밖에서 한다.
        let mut scored: Vec<(f32, i64)> = Vec::with_capacity(candidates.len());
        for (id, blob) in &candidates {
            if blob.len() != dimension * 4 {
                tracing::warn!(
                    "Skipping rulebook chunk with corrupted embedding id={id} bytes={} expected={}",
                    blob.len(),
                    dimension * 4
                );
                continue;
            }
            scored.push((
                squared_distance(&query_vector, &decode_embedding(blob)),
                *id,
            ));
        }
        scored.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        scored.truncate(top_k);

        // 3단계: 고른 청크의 본문을 읽는다.
        self.db.with_conn(|conn| {
            let mut statement = conn
                .prepare("SELECT rulebook_id, filename, text FROM rulebook_chunks WHERE id = ?1")
                .map_err(db_err)?;
            let mut results = Vec::with_capacity(scored.len());
            for (_, id) in &scored {
                let row = statement
                    .query_row([id], |row| {
                        Ok((
                            row.get::<_, Option<String>>(0)?,
                            row.get::<_, Option<String>>(1)?,
                            row.get::<_, Option<String>>(2)?,
                        ))
                    })
                    .optional()
                    .map_err(db_err)?;
                // 사이에 인덱스가 초기화됐으면 그 청크는 건너뛴다.
                let Some((Some(rulebook_id), filename, text)) = row else {
                    continue;
                };
                results.push(RulebookContext::of(
                    RulebookId::of(&rulebook_id)?,
                    filename.as_deref(),
                    text.as_deref(),
                ));
            }
            Ok(results)
        })
    }

    fn delete_rulebook_chunks(&self, rulebook_id: &RulebookId) -> AppResult<()> {
        self.db.with_conn(|conn| {
            conn.execute(
                "DELETE FROM rulebook_chunks WHERE rulebook_id = ?1",
                params![rulebook_id.value()],
            )
            .map(|_| ())
            .map_err(db_err)
        })
    }

    fn reset(&self) -> AppResult<()> {
        let metadata = self.expected_metadata();
        self.db.with_conn(|conn| {
            let tx = write_transaction(conn)?;
            tx.execute("DELETE FROM rulebook_chunks", [])
                .map_err(db_err)?;
            write_metadata(&tx, &metadata)?;
            tx.commit().map_err(db_err)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeEmbedding {
        provider: &'static str,
        model_version: &'static str,
        dimension: usize,
        vectors: Vec<(&'static str, Vec<f32>)>,
    }

    impl FakeEmbedding {
        fn alpha_beta(provider: &'static str, model_version: &'static str) -> Self {
            Self {
                provider,
                model_version,
                dimension: 2,
                vectors: vec![("alpha", vec![1.0, 0.0]), ("beta", vec![0.0, 1.0])],
            }
        }
    }

    impl EmbeddingPort for FakeEmbedding {
        fn embed(&self, text: &str) -> AppResult<Vec<f32>> {
            let lowered = text.to_lowercase();
            for (needle, vector) in &self.vectors {
                if lowered.contains(needle) {
                    return Ok(vector.clone());
                }
            }
            Ok(vec![0.5; self.dimension])
        }
        fn dimension(&self) -> usize {
            self.dimension
        }
        fn provider(&self) -> String {
            self.provider.to_string()
        }
        fn model_version(&self) -> String {
            self.model_version.to_string()
        }
    }

    fn rid(id: &str) -> RulebookId {
        RulebookId::of(id).unwrap()
    }

    fn enabled(ids: &[&str]) -> HashSet<RulebookId> {
        ids.iter().map(|id| rid(id)).collect()
    }

    fn adapter(db: &Database, embedding: FakeEmbedding) -> SqliteRulebookIndexAdapter {
        SqliteRulebookIndexAdapter::new(db.clone(), Arc::new(embedding))
    }

    fn texts(results: &[RulebookContext]) -> Vec<&str> {
        results.iter().map(|c| c.text()).collect()
    }

    fn chunk_count(db: &Database) -> i64 {
        db.with_conn(|c| {
            c.query_row("SELECT COUNT(*) FROM rulebook_chunks", [], |r| r.get(0))
                .map_err(db_err)
        })
        .unwrap()
    }

    #[test]
    fn search_returns_empty_when_enabled_set_is_empty() {
        let db = Database::open_in_memory().unwrap();
        let adapter = adapter(&db, FakeEmbedding::alpha_beta("hash", "v1"));
        assert!(adapter
            .search("anything", 3, &HashSet::new())
            .unwrap()
            .is_empty());
    }

    #[test]
    fn search_returns_empty_when_index_does_not_exist() {
        let db = Database::open_in_memory().unwrap();
        let adapter = adapter(&db, FakeEmbedding::alpha_beta("hash", "v1"));
        assert!(adapter
            .search("alpha", 3, &enabled(&["r1"]))
            .unwrap()
            .is_empty());
    }

    #[test]
    fn index_and_search_filters_by_enabled_rulebook_ids_and_respects_top_k() {
        let db = Database::open_in_memory().unwrap();
        let adapter = adapter(&db, FakeEmbedding::alpha_beta("hash", "v1"));
        adapter
            .index_rulebook_chunks(
                &rid("r1"),
                "r1.md",
                &["alpha first".to_string(), "alpha second".to_string()],
            )
            .unwrap();
        adapter
            .index_rulebook_chunks(&rid("r2"), "r2.md", &["beta only".to_string()])
            .unwrap();

        let result = adapter.search("alpha", 1, &enabled(&["r1"])).unwrap();

        assert_eq!(1, result.len());
        assert_eq!("r1", result[0].rulebook_id().value());
        assert_eq!("r1.md", result[0].filename());
        assert!(result[0].text().contains("alpha"));
    }

    #[test]
    fn search_resets_index_when_embedding_metadata_changes() {
        let db = Database::open_in_memory().unwrap();
        adapter(&db, FakeEmbedding::alpha_beta("hash", "v1"))
            .index_rulebook_chunks(&rid("r1"), "r1.md", &["alpha first".to_string()])
            .unwrap();
        assert_eq!(1, chunk_count(&db));

        let fasttext = adapter(
            &db,
            FakeEmbedding::alpha_beta("fasttext", "cc.ko.300.vec.gz"),
        );
        let result = fasttext.search("alpha", 3, &enabled(&["r1"])).unwrap();

        assert!(result.is_empty());
        assert_eq!(0, chunk_count(&db));
    }

    #[test]
    fn dimension_change_also_resets_index() {
        let db = Database::open_in_memory().unwrap();
        adapter(&db, FakeEmbedding::alpha_beta("hash", "v1"))
            .index_rulebook_chunks(&rid("r1"), "r1.md", &["alpha".to_string()])
            .unwrap();
        let wider = FakeEmbedding {
            provider: "hash",
            model_version: "v1",
            dimension: 3,
            vectors: vec![("alpha", vec![1.0, 0.0, 0.0])],
        };
        let wider = adapter(&db, wider);
        assert!(wider
            .search("alpha", 3, &enabled(&["r1"]))
            .unwrap()
            .is_empty());
        assert_eq!(0, chunk_count(&db));
    }

    fn three_dim() -> FakeEmbedding {
        FakeEmbedding {
            provider: "hash",
            model_version: "v1",
            dimension: 3,
            vectors: vec![
                ("query", vec![1.0, 0.0, 0.0]),
                ("exact", vec![1.0, 0.0, 0.0]),
                ("near", vec![0.9, 0.3, 0.0]),
                ("mid", vec![0.0, 1.0, 0.0]),
                ("far", vec![-1.0, 0.0, 0.0]),
            ],
        }
    }

    #[test]
    fn search_ranks_by_euclidean_distance() {
        let db = Database::open_in_memory().unwrap();
        let adapter = adapter(&db, three_dim());
        // 삽입 순서가 거리 순서와 다르게 한다.
        let chunks: Vec<String> = ["far", "mid", "near", "exact"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        adapter
            .index_rulebook_chunks(&rid("r1"), "r1.md", &chunks)
            .unwrap();

        let result = adapter.search("query", 2, &enabled(&["r1"])).unwrap();

        assert_eq!(vec!["exact", "near"], texts(&result));
        let all = adapter.search("query", 10, &enabled(&["r1"])).unwrap();
        assert_eq!(vec!["exact", "near", "mid", "far"], texts(&all));
    }

    #[test]
    fn search_skips_disabled_rulebooks() {
        let db = Database::open_in_memory().unwrap();
        let adapter = adapter(&db, three_dim());
        adapter
            .index_rulebook_chunks(
                &rid("on"),
                "on.md",
                &["near".to_string(), "far".to_string()],
            )
            .unwrap();
        adapter
            .index_rulebook_chunks(&rid("off"), "off.md", &["exact".to_string()])
            .unwrap();

        let result = adapter.search("query", 10, &enabled(&["on"])).unwrap();

        assert_eq!(vec!["near", "far"], texts(&result));
        assert!(result.iter().all(|c| c.rulebook_id().value() == "on"));
    }

    #[test]
    fn search_skips_corrupted_embedding_rows() {
        let db = Database::open_in_memory().unwrap();
        let adapter = adapter(&db, three_dim());
        adapter
            .index_rulebook_chunks(
                &rid("r1"),
                "r1.md",
                &["exact".to_string(), "near".to_string(), "mid".to_string()],
            )
            .unwrap();
        db.with_conn(|c| {
            c.execute(
                "UPDATE rulebook_chunks SET embedding = X'0102030405' WHERE text = 'exact'",
                [],
            )
            .map_err(db_err)?;
            c.execute(
                "UPDATE rulebook_chunks SET embedding = NULL WHERE text = 'mid'",
                [],
            )
            .map_err(db_err)
        })
        .unwrap();

        let result = adapter.search("query", 10, &enabled(&["r1"])).unwrap();

        assert_eq!(vec!["near"], texts(&result));
    }

    #[test]
    fn embeddings_are_stored_as_little_endian_f32() {
        let db = Database::open_in_memory().unwrap();
        let adapter = adapter(&db, three_dim());
        adapter
            .index_rulebook_chunks(&rid("r1"), "r1.md", &["near".to_string()])
            .unwrap();
        let blob: Vec<u8> = db
            .with_conn(|c| {
                c.query_row("SELECT embedding FROM rulebook_chunks", [], |r| r.get(0))
                    .map_err(db_err)
            })
            .unwrap();
        let mut expected = Vec::new();
        for v in [0.9_f32, 0.3, 0.0] {
            expected.extend_from_slice(&v.to_le_bytes());
        }
        assert_eq!(expected, blob);
        assert_eq!(12, blob.len());
    }

    #[test]
    fn failed_index_write_keeps_nothing_of_that_rulebook() {
        let db = Database::open_in_memory().unwrap();
        let adapter = adapter(&db, three_dim());
        adapter
            .index_rulebook_chunks(&rid("r1"), "r1.md", &["near".to_string()])
            .unwrap();
        // 청크 하나의 임베딩이 틀린 차원이면 그 룰북은 하나도 들어가지 않는다.
        struct BadSecond(FakeEmbedding);
        impl EmbeddingPort for BadSecond {
            fn embed(&self, text: &str) -> AppResult<Vec<f32>> {
                if text == "bad" {
                    return Ok(vec![1.0]);
                }
                self.0.embed(text)
            }
            fn dimension(&self) -> usize {
                self.0.dimension()
            }
            fn provider(&self) -> String {
                self.0.provider()
            }
            fn model_version(&self) -> String {
                self.0.model_version()
            }
        }
        let failing = SqliteRulebookIndexAdapter::new(db.clone(), Arc::new(BadSecond(three_dim())));
        let err = failing
            .index_rulebook_chunks(
                &rid("r2"),
                "r2.md",
                &["exact".to_string(), "bad".to_string()],
            )
            .unwrap_err();
        assert!(matches!(err, AppError::InvalidState(_)));
        assert_eq!(1, chunk_count(&db));
    }

    #[test]
    fn reset_removes_all_chunks_and_keeps_index_usable() {
        let db = Database::open_in_memory().unwrap();
        let adapter = adapter(&db, three_dim());
        adapter
            .index_rulebook_chunks(&rid("r1"), "r1.md", &["near".to_string()])
            .unwrap();

        adapter.reset().unwrap();
        assert_eq!(0, chunk_count(&db));
        assert!(adapter
            .search("query", 5, &enabled(&["r1"]))
            .unwrap()
            .is_empty());

        adapter
            .index_rulebook_chunks(&rid("r1"), "r1.md", &["mid".to_string()])
            .unwrap();
        assert_eq!(1, chunk_count(&db));
    }

    #[test]
    fn indexing_the_same_rulebook_again_appends_like_the_lucene_adapter() {
        let db = Database::open_in_memory().unwrap();
        let adapter = adapter(&db, three_dim());
        for _ in 0..2 {
            adapter
                .index_rulebook_chunks(&rid("r1"), "r1.md", &["near".to_string()])
                .unwrap();
        }
        assert_eq!(2, chunk_count(&db));
    }

    #[test]
    fn index_survives_reopening_the_database() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("omypic.db");
        {
            let db = Database::open(&path).unwrap();
            adapter(&db, three_dim())
                .index_rulebook_chunks(&rid("r1"), "r1.md", &["near".to_string()])
                .unwrap();
            // Opening removes chunks without a rulebook row, so the row has to exist.
            db.with_conn(|c| {
                c.execute(
                    "INSERT INTO rulebooks (id, filename, path, scope, question_group, enabled,
                         created_at, updated_at)
                     VALUES ('r1', 'r1.md', 'rulebooks/r1.md', 'MAIN', NULL, 1,
                         '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
                    [],
                )
                .map_err(db_err)
            })
            .unwrap();
        }
        let db = Database::open(&path).unwrap();
        let result = adapter(&db, three_dim())
            .search("query", 3, &enabled(&["r1"]))
            .unwrap();
        assert_eq!(vec!["near"], texts(&result));
    }

    #[test]
    fn delete_rulebook_chunks_removes_only_that_rulebook() {
        let db = Database::open_in_memory().unwrap();
        let adapter = adapter(&db, three_dim());
        for id in ["r1", "r2"] {
            adapter
                .index_rulebook_chunks(&rid(id), "x.md", &["near".to_string()])
                .unwrap();
        }

        adapter.delete_rulebook_chunks(&rid("r1")).unwrap();

        let result = adapter.search("query", 3, &enabled(&["r1", "r2"])).unwrap();
        assert_eq!(1, result.len());
        assert_eq!("r2", result[0].rulebook_id().value());
    }
}
