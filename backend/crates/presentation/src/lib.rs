//! axum HTTP API. 경로, JSON 필드, 상태 코드, SSE 이벤트는 Java 백엔드와 같다.

pub mod error;
mod request_guard;
mod routes;
pub mod sse;
pub mod state;

use axum::extract::DefaultBodyLimit;
use axum::http::{HeaderValue, Method, StatusCode};
use axum::middleware;
use axum::Router;
use tower_http::cors::{AllowHeaders, AllowOrigin, CorsLayer};

pub use error::ApiError;
pub use state::AppState;

/// JSON과 multipart 본문의 최대 크기(64MB). 8MB 안팎의 오디오 청크와 백업 zip이 들어온다.
pub const MAX_BODY_BYTES: usize = 64 * 1024 * 1024;

/// 모든 경로를 묶은 라우터. 서버가 이 라우터를 그대로 서빙한다.
/// CORS는 `state.allowed_origins`만 허용하고, 그 밖의 `Origin`이나 루프백이 아닌 `Host`가 붙은
/// 요청은 어느 경로든 403으로 거부한다(`request_guard`).
pub fn router(state: AppState) -> Router {
    let origins: Vec<HeaderValue> = state
        .allowed_origins
        .iter()
        .filter_map(|origin| HeaderValue::from_str(origin).ok())
        .collect();
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::PATCH,
            Method::HEAD,
            Method::OPTIONS,
        ])
        .allow_headers(AllowHeaders::mirror_request());
    let guard = middleware::from_fn_with_state(
        state.allowed_origins.clone(),
        request_guard::reject_foreign_requests,
    );
    routes::routes()
        .fallback(not_found)
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .layer(cors)
        .layer(guard)
        .with_state(state)
}

async fn not_found() -> ApiError {
    ApiError::Status(StatusCode::NOT_FOUND, "Not found".to_string())
}
