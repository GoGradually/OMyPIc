use omypic_domain::error::AppResult;
use omypic_domain::question::{QuestionGroupAggregate, QuestionGroupId};

pub trait QuestionGroupPort: Send + Sync {
    fn find_all(&self) -> AppResult<Vec<QuestionGroupAggregate>>;
    fn find_all_by_id(&self, ids: &[QuestionGroupId]) -> AppResult<Vec<QuestionGroupAggregate>>;
    fn find_by_id(&self, id: &QuestionGroupId) -> AppResult<Option<QuestionGroupAggregate>>;
    fn save(&self, group: QuestionGroupAggregate) -> AppResult<QuestionGroupAggregate>;
    fn delete_by_id(&self, id: &QuestionGroupId) -> AppResult<()>;
    fn delete_all(&self) -> AppResult<()>;
}
