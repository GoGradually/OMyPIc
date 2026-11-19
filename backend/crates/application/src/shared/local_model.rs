//! 화면이 쓰는 온디바이스 음성 모델 파일을 백엔드가 받아 보관하는 포트.
use std::path::PathBuf;

use LocalModelError::{Internal, InvalidPath, NotAllowed, NotFound, Upstream};

/// 디스크에 완성돼 있는 모델 파일 하나.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedModelFile {
    pub path: PathBuf,
    pub content_type: String,
}

/// 모델 파일을 얻지 못한 이유. 라우트가 상태 코드로 옮긴다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalModelError {
    /// 경로 조각이 비었거나 `.`, `..`이거나 허용하지 않는 문자를 담고 있다.
    InvalidPath(String),
    /// 허용 목록에 없는 저장소.
    NotAllowed(String),
    /// 상위 서버에 그 파일이 없다.
    NotFound(String),
    /// 상위 서버에 닿지 못했거나 404가 아닌 오류로 답했거나 받는 도중 끊겼다.
    Upstream(String),
    /// 로컬 디스크 쓰기 등 이쪽의 문제.
    Internal(String),
}

impl std::fmt::Display for LocalModelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (InvalidPath(message) | NotAllowed(message) | NotFound(message) | Upstream(message)
        | Internal(message)) = self;
        f.write_str(message)
    }
}

impl std::error::Error for LocalModelError {}

/// 저장소(`org/name`), 리비전, 파일 경로로 캐시된 파일을 돌려준다. 없으면 내려받아 보관한다.
/// 오래 막힐 수 있으므로 블로킹 스레드에서 부른다.
pub trait LocalModelFiles: Send + Sync {
    fn fetch(
        &self,
        repo: &str,
        revision: &str,
        file: &str,
    ) -> Result<CachedModelFile, LocalModelError>;
}
