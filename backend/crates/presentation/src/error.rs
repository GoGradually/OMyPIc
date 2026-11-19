//! 오류 응답과 요청 추출기.
//!
//! 오류 본문은 `{"message": "..."}`다. 그룹 태그 오류만 `code`와 `invalidTags`가 더 붙는다
//! (Java `ApiExceptionHandler`). 요청 JSON을 읽지 못하거나 검증에 실패하면 400이다.
//! Spring은 이 경우 기본 오류 본문을 쓰지만 이 API는 같은 `{"message"}` 모양으로 답한다.

use axum::body::Bytes;
use axum::extract::multipart::MultipartError;
use axum::extract::{FromRequest, Multipart, Request};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use omypic_domain::error::{AppError, AppResult};
use omypic_domain::shared::text_utils::java_is_blank;
use serde::de::DeserializeOwned;
use serde_json::json;

#[derive(Debug)]
pub enum ApiError {
    App(AppError),
    /// 요청 자체가 잘못됐다(JSON 파싱, 검증, 파라미터 형식).
    BadRequest(String),
    /// 본문 크기 초과 등 추출 단계에서 정해진 상태 코드.
    Status(StatusCode, String),
    /// 유스케이스가 패닉했거나 작업이 취소됐다.
    Internal,
}

impl From<AppError> for ApiError {
    fn from(error: AppError) -> Self {
        Self::App(error)
    }
}

fn message_body(message: &str) -> serde_json::Value {
    json!({ "message": message })
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        match self {
            Self::App(AppError::InvalidArgument(message) | AppError::InvalidState(message)) => (
                StatusCode::BAD_REQUEST,
                Json(message_body(&non_empty(message, "Bad request"))),
            )
                .into_response(),
            Self::App(AppError::NotFound(message)) => (
                StatusCode::NOT_FOUND,
                Json(message_body(&non_empty(message, "Not found"))),
            )
                .into_response(),
            Self::App(AppError::InvalidGroupTags {
                message,
                invalid_tags,
            }) => (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "code": "INVALID_GROUP_TAGS",
                    "message": non_empty(message, "Invalid group tags"),
                    "invalidTags": invalid_tags,
                })),
            )
                .into_response(),
            Self::App(AppError::Internal(detail)) => {
                tracing::error!(error = %detail, "internal error");
                internal_response()
            }
            Self::BadRequest(message) => {
                (StatusCode::BAD_REQUEST, Json(message_body(&message))).into_response()
            }
            Self::Status(status, message) => (status, Json(message_body(&message))).into_response(),
            Self::Internal => internal_response(),
        }
    }
}

fn internal_response() -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(message_body("Internal server error")),
    )
        .into_response()
}

fn non_empty(message: String, fallback: &str) -> String {
    if message.is_empty() {
        fallback.to_string()
    } else {
        message
    }
}

/// 유스케이스는 동기이고 오래 막힐 수 있다(LLM 호출, 임베딩 모델 내려받기). 일부 어댑터는
/// `reqwest::blocking`을 쓰므로 비동기 워커 스레드에서 부르면 안 된다. 그래서 항상 blocking
/// 풀에서 돌린다. 작업이 패닉하거나 취소되면 500이고 서버는 계속 답한다.
pub(crate) async fn blocking<T, F>(job: F) -> Result<T, ApiError>
where
    F: FnOnce() -> AppResult<T> + Send + 'static,
    T: Send + 'static,
{
    match tokio::task::spawn_blocking(job).await {
        Ok(result) => result.map_err(ApiError::from),
        Err(join_error) => {
            tracing::error!(panicked = join_error.is_panic(), "blocking task failed");
            Err(ApiError::Internal)
        }
    }
}

/// `@NotBlank`와 같다(`String.isBlank`). `null`도 비어 있는 것으로 본다.
pub(crate) fn require_not_blank(field: &str, value: &Option<String>) -> Result<(), ApiError> {
    match value {
        Some(text) if !java_is_blank(text) => Ok(()),
        _ => Err(ApiError::BadRequest(format!("{field} must not be blank"))),
    }
}

fn body_rejection(status: StatusCode) -> ApiError {
    let message = if status == StatusCode::PAYLOAD_TOO_LARGE {
        "Request body is too large"
    } else {
        "Could not read request body"
    };
    ApiError::Status(status, message.to_string())
}

/// 필수 JSON 본문. 본문이 없거나 JSON이 아니거나 모양이 맞지 않으면 400이다.
/// 오류 메시지에는 본문 내용을 넣지 않는다(API 키가 들어 있을 수 있다).
pub struct ApiJson<T>(pub T);

impl<S: Send + Sync, T: DeserializeOwned> FromRequest<S> for ApiJson<T> {
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, ApiError> {
        let bytes = Bytes::from_request(req, state)
            .await
            .map_err(|rejection| body_rejection(rejection.status()))?;
        serde_json::from_slice(&bytes)
            .map(ApiJson)
            .map_err(|_| ApiError::BadRequest("Malformed JSON request body".to_string()))
    }
}

/// 있어도 없어도 되는 JSON 본문(`@RequestBody(required = false)`). 빈 본문은 `None`이다.
pub struct OptionalJson<T>(pub Option<T>);

impl<S: Send + Sync, T: DeserializeOwned> FromRequest<S> for OptionalJson<T> {
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, ApiError> {
        let bytes = Bytes::from_request(req, state)
            .await
            .map_err(|rejection| body_rejection(rejection.status()))?;
        if bytes.iter().all(u8::is_ascii_whitespace) {
            return Ok(Self(None));
        }
        serde_json::from_slice(&bytes)
            .map(|value| Self(Some(value)))
            .map_err(|_| ApiError::BadRequest("Malformed JSON request body".to_string()))
    }
}

/// `multipart/form-data` 본문. 추출 실패도 `{"message"}`로 답한다.
pub struct ApiMultipart(pub Multipart);

impl<S: Send + Sync> FromRequest<S> for ApiMultipart {
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, ApiError> {
        Multipart::from_request(req, state)
            .await
            .map(Self)
            .map_err(|rejection| {
                ApiError::Status(
                    rejection.status(),
                    "Request is not a valid multipart/form-data".to_string(),
                )
            })
    }
}

pub(crate) fn multipart_error(error: MultipartError) -> ApiError {
    let status = error.status();
    let message = if status == StatusCode::PAYLOAD_TOO_LARGE {
        "Request body is too large"
    } else {
        "Invalid multipart request"
    };
    ApiError::Status(status, message.to_string())
}
