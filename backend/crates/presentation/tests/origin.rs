//! 사용자가 의도하지 않은 웹 페이지의 요청 거부(CORS 허용 목록, `Origin`/`Host` 검사).

mod common;

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use common::*;
use serde_json::json;

const DEV_ORIGIN: &str = "http://localhost:5173";
const EVIL_ORIGIN: &str = "https://evil.example";

#[tokio::test]
async fn preflight_from_allowed_origin_echoes_origin_and_allows_api_key_header() {
    let fixture = Fixture::new();
    let request = Request::builder()
        .method(Method::OPTIONS)
        .uri("/api/question-groups/g1")
        .header(header::HOST, "127.0.0.1:4317")
        .header(header::ORIGIN, DEV_ORIGIN)
        .header(header::ACCESS_CONTROL_REQUEST_METHOD, "DELETE")
        .header(
            header::ACCESS_CONTROL_REQUEST_HEADERS,
            "x-api-key, content-type",
        )
        .body(Body::empty())
        .unwrap();

    let reply = send(&fixture.router(), request).await;

    assert_eq!(StatusCode::OK, reply.status);
    assert_eq!(
        DEV_ORIGIN,
        reply.headers[header::ACCESS_CONTROL_ALLOW_ORIGIN]
    );
    let allowed_headers = reply.headers[header::ACCESS_CONTROL_ALLOW_HEADERS]
        .to_str()
        .unwrap()
        .to_ascii_lowercase();
    assert!(allowed_headers.contains("x-api-key"), "{allowed_headers}");
    assert!(
        allowed_headers.contains("content-type"),
        "{allowed_headers}"
    );
    assert!(reply.headers[header::ACCESS_CONTROL_ALLOW_METHODS]
        .to_str()
        .unwrap()
        .contains("DELETE"));
}

#[tokio::test]
async fn every_default_origin_is_allowed() {
    let fixture = Fixture::new();
    for origin in DEFAULT_ALLOWED_ORIGINS {
        let request = Request::builder()
            .uri("/api/question-groups")
            .header(header::HOST, "127.0.0.1:4317")
            .header(header::ORIGIN, origin)
            .body(Body::empty())
            .unwrap();

        let reply = send(&fixture.router(), request).await;

        assert_eq!(StatusCode::OK, reply.status, "{origin}");
        assert_eq!(origin, reply.headers[header::ACCESS_CONTROL_ALLOW_ORIGIN]);
    }
}

#[tokio::test]
async fn preflight_from_unknown_origin_is_forbidden() {
    let fixture = Fixture::new();
    let request = Request::builder()
        .method(Method::OPTIONS)
        .uri("/api/data-transfer/export")
        .header(header::ORIGIN, EVIL_ORIGIN)
        .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
        .body(Body::empty())
        .unwrap();

    let reply = send(&fixture.router(), request).await;

    assert_eq!(StatusCode::FORBIDDEN, reply.status);
    assert!(!reply
        .headers
        .contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN));
}

#[tokio::test]
async fn cross_origin_form_post_to_import_is_forbidden_and_changes_nothing() {
    let fixture = Fixture::new();
    fixture.add_group(group("g1", "Travel", &["travel"], vec![]));
    let mut request = multipart(
        Method::POST,
        "/api/data-transfer/import",
        &[Part::File {
            name: "file",
            filename: "backup.zip",
            bytes: b"PK\x05\x06\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
        }],
    );
    request
        .headers_mut()
        .insert(header::ORIGIN, EVIL_ORIGIN.parse().unwrap());

    let reply = send(&fixture.router(), request).await;

    assert_eq!(StatusCode::FORBIDDEN, reply.status);
    assert_eq!(json!({"message": "Origin not allowed"}), reply.json());
    assert_eq!(1, fixture.questions.groups().len());
}

#[tokio::test]
async fn requests_from_unknown_origin_are_forbidden_for_every_method() {
    let fixture = Fixture::new();
    let router = fixture.router();
    for (method, uri) in [
        (Method::GET, "/api/data-transfer/export"),
        (Method::GET, "/api/voice/sessions/any/events"),
        (Method::DELETE, "/api/question-groups/g1"),
        (Method::PUT, "/api/rulebooks/r1/toggle?enabled=false"),
        (Method::GET, "/actuator/health"),
        (Method::GET, "/no/such/path"),
    ] {
        for origin in [EVIL_ORIGIN, "null", "http://localhost:5174"] {
            let request = Request::builder()
                .method(method.clone())
                .uri(uri)
                .header(header::ORIGIN, origin)
                .body(Body::empty())
                .unwrap();

            let reply = send(&router, request).await;

            assert_eq!(
                StatusCode::FORBIDDEN,
                reply.status,
                "{method} {uri} {origin}"
            );
            assert_eq!(json!({"message": "Origin not allowed"}), reply.json());
        }
    }
}

#[tokio::test]
async fn requests_without_origin_are_allowed() {
    let fixture = Fixture::new();
    let reply = send(&fixture.router(), get("/api/question-groups")).await;
    assert_eq!(StatusCode::OK, reply.status);
}

#[tokio::test]
async fn foreign_host_header_is_forbidden() {
    let fixture = Fixture::new();
    for host in [
        "evil.example",
        "evil.example:4317",
        "localhost.evil.example",
        "10.0.0.1:4317",
    ] {
        let request = Request::builder()
            .uri("/api/question-groups")
            .header(header::HOST, host)
            .body(Body::empty())
            .unwrap();

        let reply = send(&fixture.router(), request).await;

        assert_eq!(StatusCode::FORBIDDEN, reply.status, "{host}");
        assert_eq!(json!({"message": "Host not allowed"}), reply.json());
    }
}

#[tokio::test]
async fn loopback_host_headers_are_allowed_with_any_port() {
    let fixture = Fixture::new();
    for host in [
        "127.0.0.1",
        "127.0.0.1:4317",
        "localhost",
        "localhost:51234",
        "LocalHost:4317",
        "[::1]",
        "[::1]:4317",
    ] {
        let request = Request::builder()
            .uri("/api/question-groups")
            .header(header::HOST, host)
            .header(header::ORIGIN, DEV_ORIGIN)
            .body(Body::empty())
            .unwrap();

        let reply = send(&fixture.router(), request).await;

        assert_eq!(StatusCode::OK, reply.status, "{host}");
    }
}
