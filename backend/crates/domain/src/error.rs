use thiserror::Error;

/// 백엔드 전체에서 공유하는 오류. 각 변형은 Java 예외에 대응한다.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AppError {
    /// `IllegalArgumentException`
    #[error("{0}")]
    InvalidArgument(String),
    /// `IllegalStateException`
    #[error("{0}")]
    InvalidState(String),
    /// `NoSuchElementException`
    #[error("{0}")]
    NotFound(String),
    /// `InvalidGroupTagsException`
    #[error("{message}")]
    InvalidGroupTags {
        message: String,
        invalid_tags: Vec<String>,
    },
    /// I/O 및 예상하지 못한 실패 (HTTP 500).
    #[error("{0}")]
    Internal(String),
}

pub type AppResult<T> = Result<T, AppError>;
