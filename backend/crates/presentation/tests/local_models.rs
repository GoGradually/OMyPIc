//! `/api/local-models/...` 경로: 상위 서버(httpmock)에서 받아 보관한 파일을 내보낸다.

mod common;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use common::*;
use httpmock::prelude::*;
use omypic_infrastructure::config::LocalModelsConfig;
use omypic_infrastructure::local_models::LocalModelCache;

const PATH: &str = "/api/local-models/Xenova/whisper-base.en/resolve/main/onnx/model.onnx";
const UPSTREAM_PATH: &str = "/Xenova/whisper-base.en/resolve/main/onnx/model.onnx";

struct Setup {
    fixture: Fixture,
    server: MockServer,
    _dir: tempfile::TempDir,
}

fn setup() -> Setup {
    let dir = tempfile::tempdir().unwrap();
    let server = MockServer::start();
    let config = LocalModelsConfig {
        upstream: server.base_url(),
        allowed_repos: vec!["Xenova/whisper-base.en".to_string()],
    };
    let mut fixture = Fixture::new();
    fixture.state.local_models = Arc::new(LocalModelCache::new(&config, dir.path()));
    Setup {
        fixture,
        server,
        _dir: dir,
    }
}

fn with_header(method: Method, uri: &str, name: header::HeaderName, value: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(name, value)
        .body(Body::empty())
        .unwrap()
}

#[tokio::test]
async fn serves_the_file_with_length_and_type_and_caches_it() {
    let setup = setup();
    let upstream = setup.server.mock(|when, then| {
        when.method(GET).path(UPSTREAM_PATH);
        then.status(200).body("0123456789");
    });

    let first = send(&setup.fixture.router(), get(PATH)).await;
    let second = send(&setup.fixture.router(), get(PATH)).await;

    assert_eq!(StatusCode::OK, first.status);
    assert_eq!(b"0123456789".to_vec(), first.body);
    assert_eq!("10", first.headers[header::CONTENT_LENGTH]);
    assert_eq!(
        "application/octet-stream",
        first.headers[header::CONTENT_TYPE]
    );
    assert_eq!("bytes", first.headers[header::ACCEPT_RANGES]);
    assert_eq!(first.body, second.body);
    assert_eq!(1, upstream.hits());
}

#[tokio::test]
async fn json_files_are_served_as_json() {
    let setup = setup();
    setup.server.mock(|when, then| {
        when.method(GET)
            .path("/Xenova/whisper-base.en/resolve/main/config.json");
        then.status(200).body("{\"a\":1}");
    });

    let reply = send(
        &setup.fixture.router(),
        get("/api/local-models/Xenova/whisper-base.en/resolve/main/config.json"),
    )
    .await;

    assert_eq!("application/json", reply.headers[header::CONTENT_TYPE]);
    assert_eq!(serde_json::json!({"a": 1}), reply.json());
}

#[tokio::test]
async fn head_returns_headers_without_a_body() {
    let setup = setup();
    setup.server.mock(|when, then| {
        when.method(GET).path(UPSTREAM_PATH);
        then.status(200).body("0123456789");
    });

    let reply = send(
        &setup.fixture.router(),
        Request::builder()
            .method(Method::HEAD)
            .uri(PATH)
            .body(Body::empty())
            .unwrap(),
    )
    .await;

    assert_eq!(StatusCode::OK, reply.status);
    assert_eq!("10", reply.headers[header::CONTENT_LENGTH]);
    assert!(reply.body.is_empty());
}

#[tokio::test]
async fn range_request_returns_the_requested_bytes() {
    let setup = setup();
    setup.server.mock(|when, then| {
        when.method(GET).path(UPSTREAM_PATH);
        then.status(200).body("0123456789");
    });
    let router = setup.fixture.router();

    let first_byte = send(
        &router,
        with_header(Method::GET, PATH, header::RANGE, "bytes=0-0"),
    )
    .await;
    let tail = send(
        &router,
        with_header(Method::GET, PATH, header::RANGE, "bytes=-4"),
    )
    .await;
    let beyond = send(
        &router,
        with_header(Method::GET, PATH, header::RANGE, "bytes=10-"),
    )
    .await;

    assert_eq!(StatusCode::PARTIAL_CONTENT, first_byte.status);
    assert_eq!(b"0".to_vec(), first_byte.body);
    assert_eq!("bytes 0-0/10", first_byte.headers[header::CONTENT_RANGE]);
    assert_eq!("1", first_byte.headers[header::CONTENT_LENGTH]);
    assert_eq!(StatusCode::PARTIAL_CONTENT, tail.status);
    assert_eq!(b"6789".to_vec(), tail.body);
    assert_eq!("bytes 6-9/10", tail.headers[header::CONTENT_RANGE]);
    assert_eq!(StatusCode::RANGE_NOT_SATISFIABLE, beyond.status);
    assert_eq!("bytes */10", beyond.headers[header::CONTENT_RANGE]);
}

#[tokio::test]
async fn large_file_is_streamed_whole() {
    let setup = setup();
    let body: Vec<u8> = (0..3_000_000_u32).map(|n| (n % 251) as u8).collect();
    setup.server.mock(|when, then| {
        when.method(GET).path(UPSTREAM_PATH);
        then.status(200).body(body.clone());
    });

    let reply = send(&setup.fixture.router(), get(PATH)).await;

    assert_eq!(StatusCode::OK, reply.status);
    assert_eq!(body, reply.body);
}

#[tokio::test]
async fn rejects_repo_outside_allowlist_with_403() {
    let setup = setup();
    let upstream = setup.server.mock(|when, then| {
        when.method(GET);
        then.status(200).body("x");
    });

    let reply = send(
        &setup.fixture.router(),
        get("/api/local-models/evil/model/resolve/main/config.json"),
    )
    .await;

    assert_eq!(StatusCode::FORBIDDEN, reply.status);
    assert_eq!(0, upstream.hits());
}

#[tokio::test]
async fn rejects_path_traversal_with_400() {
    let setup = setup();
    let upstream = setup.server.mock(|when, then| {
        when.method(GET);
        then.status(200).body("x");
    });
    let router = setup.fixture.router();

    for uri in [
        "/api/local-models/Xenova/whisper-base.en/resolve/main/../x",
        "/api/local-models/Xenova/whisper-base.en/resolve/main/%2e%2e/x",
        "/api/local-models/Xenova/whisper-base.en/resolve/main/a/%2E%2E/x",
        "/api/local-models/Xenova/whisper-base.en/resolve/main/a%2F..%2Fx",
        "/api/local-models/Xenova/whisper-base.en/resolve/%2e%2e/x",
        "/api/local-models/Xenova/%2e%2e/resolve/main/x",
        "/api/local-models/Xenova/whisper-base.en/resolve/main/a//b",
        "/api/local-models/Xenova/whisper-base.en/resolve/main/a%5Cb",
    ] {
        let reply = send(&router, get(uri)).await;

        assert_eq!(StatusCode::BAD_REQUEST, reply.status, "{uri}");
    }
    assert_eq!(0, upstream.hits());
}

#[tokio::test]
async fn upstream_404_maps_to_404() {
    let setup = setup();
    setup.server.mock(|when, then| {
        when.method(GET);
        then.status(404);
    });

    let reply = send(&setup.fixture.router(), get(PATH)).await;

    assert_eq!(StatusCode::NOT_FOUND, reply.status);
}

#[tokio::test]
async fn upstream_500_maps_to_502() {
    let setup = setup();
    setup.server.mock(|when, then| {
        when.method(GET);
        then.status(500);
    });

    let reply = send(&setup.fixture.router(), get(PATH)).await;

    assert_eq!(StatusCode::BAD_GATEWAY, reply.status);
}

#[tokio::test]
async fn cors_exposes_range_headers_to_allowed_origins() {
    let setup = setup();
    setup.server.mock(|when, then| {
        when.method(GET).path(UPSTREAM_PATH);
        then.status(200).body("0123456789");
    });
    let request = Request::builder()
        .uri(PATH)
        .header(header::ORIGIN, "tauri://localhost")
        .body(Body::empty())
        .unwrap();

    let reply = send(&setup.fixture.router(), request).await;

    assert_eq!(StatusCode::OK, reply.status);
    let exposed = reply.headers[header::ACCESS_CONTROL_EXPOSE_HEADERS]
        .to_str()
        .unwrap()
        .to_ascii_lowercase();
    assert!(exposed.contains("content-range"), "{exposed}");
}
