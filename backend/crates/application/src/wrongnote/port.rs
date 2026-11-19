use omypic_domain::error::AppResult;
use omypic_domain::wrongnote::{WrongNote, WrongNoteId};

pub trait WrongNotePort: Send + Sync {
    fn find_all(&self) -> AppResult<Vec<WrongNote>>;
    fn find_by_pattern(&self, pattern: &str) -> AppResult<Option<WrongNote>>;
    fn save(&self, note: WrongNote) -> AppResult<WrongNote>;
    fn delete_by_id(&self, id: &WrongNoteId) -> AppResult<()>;
    fn delete_all(&self) -> AppResult<()>;
}

pub trait WrongNoteRecentQueuePort: Send + Sync {
    fn load_global_queue(&self) -> AppResult<Vec<String>>;
    fn save_global_queue(&self, patterns: &[String]) -> AppResult<()>;
    fn clear_global_queue(&self) -> AppResult<()>;
}
