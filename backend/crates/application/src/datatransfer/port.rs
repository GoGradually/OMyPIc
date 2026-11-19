use omypic_domain::error::AppResult;
use omypic_domain::question::QuestionGroupAggregate;
use omypic_domain::rulebook::Rulebook;
use omypic_domain::wrongnote::WrongNote;

/// 룰북 하나와 색인할 본문 조각들. 룰북의 `path`는 이미 저장한 새 파일이다.
#[derive(Debug, Clone)]
pub struct ReplacementRulebook {
    pub rulebook: Rulebook,
    pub chunks: Vec<String>,
}

/// 가져오기가 저장소에 남길 데이터 전부.
#[derive(Debug, Clone, Default)]
pub struct DataReplacement {
    pub question_groups: Vec<QuestionGroupAggregate>,
    pub rulebooks: Vec<ReplacementRulebook>,
    pub wrong_notes: Vec<WrongNote>,
    pub wrong_note_queue: Vec<String>,
}

/// 질문 그룹, 룰북 메타데이터, 오답노트, 오답 큐, 룰북 색인을 한 단위로 바꾼다.
///
/// 계약: `Ok`이면 이전 내용은 모두 지워지고 `replacement`만 남는다. `Err`이면 아무것도
/// 바뀌지 않았다(프로세스가 도중에 죽어도 마찬가지다). 실패할 수 있는 준비(임베딩 계산 등)는
/// 무엇이든 지우기 전에 끝낸다. 룰북 원본 파일은 다루지 않는다.
pub trait DataReplacementPort: Send + Sync {
    fn replace_all(&self, replacement: &DataReplacement) -> AppResult<()>;
}
