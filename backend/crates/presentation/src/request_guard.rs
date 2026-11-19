//! 사용자가 의도하지 않은 웹 페이지의 요청을 막는다.
//!
//! 서버는 `127.0.0.1`에서만 듣지만 브라우저에 열린 아무 페이지나 이 주소로 요청을 보낼 수 있다.
//! CORS만으로는 막지 못하는 요청(미리 묻지 않는 단순 POST 폼 전송 등)이 있으므로 응답을 만들기
//! 전에 직접 검사한다.
//! - `Origin`이 있는데 허용 목록에 없으면 403. 없으면(같은 origin 탐색, curl, 시험) 통과.
//! - `Host`가 `127.0.0.1`, `localhost`, `[::1]`(포트는 아무거나)가 아니면 403. DNS 리바인딩으로
//!   다른 이름을 이 주소로 돌려도 브라우저는 그 이름을 `Host`에 넣는다.

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::error::ApiError;

pub(crate) async fn reject_foreign_requests(
    State(allowed_origins): State<Arc<Vec<String>>>,
    request: Request,
    next: Next,
) -> Response {
    if !host_is_loopback(request.headers()) {
        return forbidden("Host not allowed");
    }
    if let Some(origin) = request.headers().get(header::ORIGIN) {
        let allowed = origin
            .to_str()
            .is_ok_and(|origin| allowed_origins.iter().any(|allowed| allowed == origin));
        if !allowed {
            return forbidden("Origin not allowed");
        }
    }
    next.run(request).await
}

fn forbidden(message: &str) -> Response {
    ApiError::Status(StatusCode::FORBIDDEN, message.to_string()).into_response()
}

/// `Host`가 없으면 통과한다(브라우저는 항상 보낸다).
fn host_is_loopback(headers: &HeaderMap) -> bool {
    let Some(host) = headers.get(header::HOST) else {
        return true;
    };
    let Ok(host) = host.to_str() else {
        return false;
    };
    let name = if let Some(rest) = host.strip_prefix('[') {
        match rest.split_once(']') {
            Some((name, port)) if port.is_empty() || is_port(port) => name,
            _ => return false,
        }
    } else {
        match host.split_once(':') {
            Some((name, port)) if is_port(port) => name,
            Some(_) => return false,
            None => host,
        }
    };
    name == "127.0.0.1" || name == "::1" || name.eq_ignore_ascii_case("localhost")
}

fn is_port(text: &str) -> bool {
    text.strip_prefix(':')
        .unwrap_or(text)
        .parse::<u16>()
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn with_host(host: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HeaderValue::from_str(host).unwrap());
        headers
    }

    #[test]
    fn host_check_accepts_only_loopback_names() {
        for host in [
            "127.0.0.1",
            "localhost:1",
            "[::1]",
            "[::1]:65535",
            "LOCALHOST",
        ] {
            assert!(host_is_loopback(&with_host(host)), "{host}");
        }
        for host in [
            "127.0.0.1.evil.example",
            "127.0.0.2",
            "localhost:",
            "localhost:abc",
            "localhost:4317:1",
            "[::1]x",
            "[::2]:4317",
            "",
        ] {
            assert!(!host_is_loopback(&with_host(host)), "{host}");
        }
        assert!(host_is_loopback(&HeaderMap::new()));
    }
}
