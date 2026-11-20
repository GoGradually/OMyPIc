//! 화면이 쓰는 온디바이스 음성 모델 파일을 백엔드가 받아 보관하는 포트.
use std::io::Read;
use std::path::PathBuf;

use LocalModelError::{Internal, InvalidPath, NotAllowed, NotFound, Upstream};

/// 디스크에 완성돼 있는 모델 파일 하나.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedModelFile {
    pub path: PathBuf,
    pub content_type: String,
}

/// 파일 내용 대신 크기 같은 정보만 묻는 요청(HEAD, 또는 `Range` 헤더가 붙은 요청).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModelFileProbe {
    pub head: bool,
    /// 받은 `Range` 헤더 값 그대로.
    pub range: Option<String>,
}

/// `open`의 결과.
pub enum ModelFile {
    /// 디스크에 다 받아 둔 파일.
    Cached(CachedModelFile),
    /// 지금 받고 있는 파일(받기는 이 요청과 상관없이 끝까지 계속된다).
    Downloading(DownloadingModelFile),
    /// 캐시에 없고 받는 중도 아닌 파일의 정보 요청에 상위 서버가 한 답. 캐시에 넣지 않는다.
    Upstream(UpstreamModelReply),
}

/// 받는 중인 파일을 처음부터 읽는 본문. 아직 받지 않은 부분은 받을 때까지 기다리고, 받기가
/// 실패하면 읽기가 오류로 끝난다. 읽기가 멈출 수 있으므로 블로킹 스레드에서 읽는다.
pub struct DownloadingModelFile {
    pub content_type: String,
    /// 상위 서버가 알려 준 전체 크기.
    pub length: Option<u64>,
    pub body: Box<dyn Read + Send>,
}

/// 상위 서버의 답을 그대로 옮긴 것. `body`는 블로킹 스레드에서 읽고 버린다.
pub struct UpstreamModelReply {
    pub status: u16,
    pub content_type: String,
    pub content_length: Option<u64>,
    pub content_range: Option<String>,
    pub body: Box<dyn Read + Send>,
}

impl std::fmt::Debug for ModelFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cached(file) => f.debug_tuple("Cached").field(file).finish(),
            Self::Downloading(file) => f
                .debug_struct("Downloading")
                .field("content_type", &file.content_type)
                .field("length", &file.length)
                .finish_non_exhaustive(),
            Self::Upstream(reply) => f
                .debug_struct("Upstream")
                .field("status", &reply.status)
                .field("content_length", &reply.content_length)
                .field("content_range", &reply.content_range)
                .finish_non_exhaustive(),
        }
    }
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

/// 저장소(`org/name`), 리비전, 파일 경로의 모델 파일을 연다.
///
/// - 캐시에 있으면 `Cached`.
/// - 받는 중이면 그 받기를 따라 읽는 `Downloading`(같은 파일을 두 번 받지 않는다).
/// - 없고 `probe`가 있으면 상위 서버에 같은 것을 물어 `Upstream`으로 돌려준다(캐시하지 않는다).
/// - 없고 `probe`가 없으면 받기를 시작하고, 상위 서버의 응답 헤더가 오면 `Downloading`을 돌려준다.
///
/// 상위 서버의 응답 헤더를 기다리며 막히므로 블로킹 스레드에서 부른다.
pub trait LocalModelFiles: Send + Sync {
    fn open(
        &self,
        repo: &str,
        revision: &str,
        file: &str,
        probe: Option<&ModelFileProbe>,
    ) -> Result<ModelFile, LocalModelError>;
}
