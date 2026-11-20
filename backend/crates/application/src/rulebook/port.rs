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
    /// 새 파일로 저장한다. 같은 `filename`을 여러 번 저장해도 경로는 매번 다르고,
    /// 이미 있는 파일을 덮어쓰지 않는다.
    fn store(&self, filename: &str, bytes: &[u8]) -> AppResult<StoredRulebookFile>;
    /// 파일이 없으면 `AppError::NotFound`.
    fn read_text(&self, path: &str) -> AppResult<String>;
    /// 파일이 없으면 `AppError::NotFound`.
    fn read_bytes(&self, path: &str) -> AppResult<Vec<u8>>;
    /// `store`가 돌려준 경로의 파일을 지운다. 이미 없으면 아무것도 하지 않는다.
    fn delete(&self, path: &str) -> AppResult<()>;
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
    /// 룰북 하나의 조각을 모두 지운다. 조각이 없으면 아무것도 하지 않는다.
    fn delete_rulebook_chunks(&self, rulebook_id: &RulebookId) -> AppResult<()>;
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
