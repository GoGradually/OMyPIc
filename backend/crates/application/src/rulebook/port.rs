use std::collections::HashSet;

use omypic_domain::error::AppResult;
use omypic_domain::rulebook::{Rulebook, RulebookContext, RulebookId};

use super::model::StoredRulebookFile;

pub trait EmbeddingPort: Send + Sync {
    fn embed(&self, text: &str) -> AppResult<Vec<f32>>;
    fn dimension(&self) -> usize;
    fn provider(&self) -> String {
        "unknown".to_string()
    }
    fn model_version(&self) -> String {
        "unknown".to_string()
    }
}

/// 트랜잭션을 지원하지 못하는 파일 저장소.
pub trait RulebookFileStore: Send + Sync {
    fn store(&self, filename: &str, bytes: &[u8]) -> AppResult<StoredRulebookFile>;
    fn read_text(&self, path: &str) -> AppResult<String>;
    fn read_bytes(&self, path: &str) -> AppResult<Vec<u8>>;
    fn clear_all(&self) -> AppResult<()>;
}

pub trait RulebookIndexPort: Send + Sync {
    fn index_rulebook_chunks(
        &self,
        rulebook_id: &RulebookId,
        filename: &str,
        chunks: &[String],
    ) -> AppResult<()>;
    fn search(
        &self,
        query: &str,
        top_k: usize,
        enabled_rulebook_ids: &HashSet<RulebookId>,
    ) -> AppResult<Vec<RulebookContext>>;
    fn reset(&self) -> AppResult<()>;
}

/// 메타데이터 저장소. 트랜잭션을 지원한다.
pub trait RulebookPort: Send + Sync {
    fn find_all(&self) -> AppResult<Vec<Rulebook>>;
    fn find_by_id(&self, id: &RulebookId) -> AppResult<Option<Rulebook>>;
    fn save(&self, rulebook: Rulebook) -> AppResult<Rulebook>;
    fn delete_by_id(&self, id: &RulebookId) -> AppResult<()>;
    fn delete_all(&self) -> AppResult<()>;
}
