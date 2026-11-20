//! `/api/local-models/...` 경로: 상위 서버(httpmock 또는 직접 답하는 가짜 서버)에서 받아 보관한
//! 파일을 내보낸다.

mod common;

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use axum::body::Body;
use axum::http::{header, Method, Request, Response, StatusCode};
use axum::Router;
use common::*;
use http_body_util::BodyExt;
use httpmock::prelude::*;
use omypic_infrastructure::config::LocalModelsConfig;
use omypic_infrastructure::local_models::LocalModelCache;
use tower::ServiceExt;

const PATH: &str = "/api/local-models/Xenova/whisper-base.en/resolve/main/onnx/model.onnx";
const UPSTREAM_PATH: &str = "/Xenova/whisper-base.en/resolve/main/onnx/model.onnx";
const WAIT: Duration = Duration::from_secs(5);
const UNREACHABLE_UPSTREAM: &str = "http://127.0.0.1:1";

struct Setup {
    fixture: Fixture,
    server: MockServer,
    dir: tempfile::TempDir,
}

fn fixture_for(upstream: &str, dir: &Path) -> Fixture {
    let config = LocalModelsConfig {
        upstream: upstream.to_string(),
        allowed_repos: vec!["Xenova/whisper-base.en".to_string()],
    };
    let mut fixture = Fixture::new();
    fixture.state.local_models = Arc::new(LocalModelCache::new(&config, dir));
    fixture
}

fn setup() -> Setup {
    let dir = tempfile::tempdir().unwrap();
    let server = MockServer::start();
    let fixture = fixture_for(&server.base_url(), dir.path());
    Setup {
        fixture,
        server,
        dir,
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

fn head(uri: &str) -> Request<Body> {
    Request::builder()
        .method(Method::HEAD)
        .uri(uri)
        .body(Body::empty())
        .unwrap()
}

fn files_under(dir: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, &mut out);
    out
}

fn cached_file(dir: &Path) -> PathBuf {
    dir.join("local-models/Xenova/whisper-base.en/main/onnx/model.onnx")
}

async fn wait_until(what: &str, check: impl Fn() -> bool) {
    let deadline = tokio::time::Instant::now() + WAIT;
    while !check() {
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting: {what}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// 응답 헤더만 받는다(본문은 아직 읽지 않는다).
async fn open(router: &Router, request: Request<Body>) -> Response<Body> {
    tokio::time::timeout(WAIT, router.clone().oneshot(request))
        .await
        .expect("response headers did not arrive in time")
        .unwrap()
}

/// 본문 조각 하나. `None`은 본문 끝.
async fn next_chunk(body: &mut Body) -> Option<Result<Vec<u8>, axum::Error>> {
    let frame = tokio::time::timeout(WAIT, body.frame())
        .await
        .expect("no body data in time")?;
    Some(frame.map(|frame| frame.into_data().unwrap_or_default().to_vec()))
}

async fn read_to_end(body: &mut Body) -> Result<Vec<u8>, axum::Error> {
    let mut bytes = Vec::new();
    while let Some(chunk) = next_chunk(body).await {
        bytes.extend(chunk?);
    }
    Ok(bytes)
}

/// 시험이 열 때까지 막는 문.
#[derive(Clone, Default)]
struct Gate(Arc<(Mutex<bool>, Condvar)>);

impl Gate {
    fn open(&self) {
        *self.0 .0.lock().unwrap() = true;
        self.0 .1.notify_all();
    }

    fn wait(&self) {
        let mut open = self.0 .0.lock().unwrap();
        while !*open {
            open = self.0 .1.wait(open).unwrap();
        }
    }
}

/// 요청마다 `respond`로 직접 답하는 상위 서버. 받은 요청의 헤더 부분을 소문자로 기록한다.
struct FakeUpstream {
    url: String,
    requests: Arc<Mutex<Vec<String>>>,
}

impl FakeUpstream {
    fn start(respond: impl Fn(&str, &mut std::net::TcpStream) + Send + Sync + 'static) -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded = requests.clone();
        let respond = Arc::new(respond);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let recorded = recorded.clone();
                let respond = respond.clone();
                std::thread::spawn(move || {
                    let mut head = Vec::new();
                    let mut byte = [0_u8; 1];
                    while !head.ends_with(b"\r\n\r\n") {
                        match stream.read(&mut byte) {
                            Ok(1) => head.push(byte[0]),
                            _ => return,
                        }
                    }
                    let head = String::from_utf8_lossy(&head).to_ascii_lowercase();
                    recorded.lock().unwrap().push(head.clone());
                    respond(&head, &mut stream);
                });
            }
        });
        Self { url, requests }
    }

    fn requests(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }
}

/// `Content-Length: 10`으로 `01234`를 보내고, 문이 열리면 `then`을 보내고 끊는다.
fn split_upstream(gate: &Gate, then: &'static [u8]) -> FakeUpstream {
    let gate = gate.clone();
    FakeUpstream::start(move |_, stream| {
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\nConnection: close\r\n\r\n01234")
            .unwrap();
        stream.flush().unwrap();
        gate.wait();
        let _ = stream.write_all(then);
    })
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
    assert_eq!("10", second.headers[header::CONTENT_LENGTH]);
    assert_eq!(1, upstream.hits());
    // 본문은 길이만큼 보내면 끝나고, 파일을 제자리에 옮기는 것은 그 직후다.
    wait_until("the file is cached", || {
        cached_file(setup.dir.path()).is_file()
    })
    .await;
    assert_eq!(
        b"0123456789".to_vec(),
        std::fs::read(cached_file(setup.dir.path())).unwrap()
    );
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
async fn head_of_a_cached_file_returns_headers_without_a_body() {
    let setup = setup();
    let upstream = setup.server.mock(|when, then| {
        when.method(GET).path(UPSTREAM_PATH);
        then.status(200).body("0123456789");
    });
    send(&setup.fixture.router(), get(PATH)).await;

    let reply = send(&setup.fixture.router(), head(PATH)).await;

    assert_eq!(StatusCode::OK, reply.status);
    assert_eq!("10", reply.headers[header::CONTENT_LENGTH]);
    assert!(reply.body.is_empty());
    assert_eq!(1, upstream.hits());
}

#[tokio::test]
async fn range_request_on_a_cached_file_returns_the_requested_bytes() {
    let setup = setup();
    let upstream = setup.server.mock(|when, then| {
        when.method(GET).path(UPSTREAM_PATH);
        then.status(200).body("0123456789");
    });
    let router = setup.fixture.router();
    send(&router, get(PATH)).await;

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
    assert_eq!(1, upstream.hits(), "캐시된 파일은 상위 서버에 묻지 않는다");
}

#[tokio::test]
async fn large_file_is_streamed_whole() {
    let setup = setup();
    let body: Vec<u8> = (0..3_000_000_u32).map(|n| (n % 251) as u8).collect();
    setup.server.mock(|when, then| {
        when.method(GET).path(UPSTREAM_PATH);
        then.status(200).body(body.clone());
    });

    let first = send(&setup.fixture.router(), get(PATH)).await;
    let cached = send(&setup.fixture.router(), get(PATH)).await;

    assert_eq!(StatusCode::OK, first.status);
    assert_eq!(body, first.body);
    assert_eq!(body, cached.body);
}

#[tokio::test]
async fn first_download_sends_headers_and_bytes_before_the_upstream_finishes() {
    let dir = tempfile::tempdir().unwrap();
    let gate = Gate::default();
    let upstream = split_upstream(&gate, b"56789");
    let router = fixture_for(&upstream.url, dir.path()).router();

    let response = open(&router, get(PATH)).await;
    assert_eq!(StatusCode::OK, response.status());
    assert_eq!("10", response.headers()[header::CONTENT_LENGTH]);
    assert_eq!(
        "application/octet-stream",
        response.headers()[header::CONTENT_TYPE]
    );
    let mut body = response.into_body();
    assert_eq!(
        b"01234".to_vec(),
        next_chunk(&mut body).await.unwrap().unwrap()
    );
    assert!(!cached_file(dir.path()).exists(), "아직 다 받지 않았다");

    gate.open();
    assert_eq!(b"56789".to_vec(), read_to_end(&mut body).await.unwrap());
    // 본문은 길이만큼 보내면 끝나고, 파일을 제자리에 옮기는 것은 그 직후다.
    wait_until("the file is cached", || cached_file(dir.path()).is_file()).await;
    assert_eq!(
        b"0123456789".to_vec(),
        std::fs::read(cached_file(dir.path())).unwrap()
    );
    assert_eq!(1, files_under(dir.path()).len(), "임시 파일이 남지 않는다");
}

#[tokio::test]
async fn a_client_that_leaves_mid_download_still_fills_the_cache() {
    let dir = tempfile::tempdir().unwrap();
    let gate = Gate::default();
    let upstream = split_upstream(&gate, b"56789");
    let router = fixture_for(&upstream.url, dir.path()).router();

    let mut body = open(&router, get(PATH)).await.into_body();
    next_chunk(&mut body).await.unwrap().unwrap();
    drop(body);
    gate.open();

    let target = cached_file(dir.path());
    wait_until("the file is cached", || target.is_file()).await;
    let again = send(&router, get(PATH)).await;
    assert_eq!(b"0123456789".to_vec(), again.body);
    assert_eq!(1, upstream.requests().len());
}

#[tokio::test]
async fn failure_mid_download_ends_the_response_with_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let gate = Gate::default();
    let upstream = split_upstream(&gate, b"");
    let router = fixture_for(&upstream.url, dir.path()).router();

    let response = open(&router, get(PATH)).await;
    assert_eq!(StatusCode::OK, response.status());
    assert_eq!("10", response.headers()[header::CONTENT_LENGTH]);
    let mut body = response.into_body();
    assert_eq!(
        b"01234".to_vec(),
        next_chunk(&mut body).await.unwrap().unwrap()
    );
    gate.open();

    let rest = read_to_end(&mut body).await;
    assert!(rest.is_err(), "본문이 오류로 끝난다: {rest:?}");
    assert!(
        files_under(dir.path()).is_empty(),
        "완성 파일도 임시 파일도 없다: {:?}",
        files_under(dir.path())
    );
}

#[tokio::test]
async fn concurrent_requests_share_one_download_and_get_the_whole_file() {
    let dir = tempfile::tempdir().unwrap();
    let gate = Gate::default();
    let upstream = split_upstream(&gate, b"56789");
    let router = fixture_for(&upstream.url, dir.path()).router();

    let first = open(&router, get(PATH)).await;
    let second = open(&router, get(PATH)).await;
    assert_eq!("10", second.headers()[header::CONTENT_LENGTH]);
    gate.open();

    let (mut first, mut second) = (first.into_body(), second.into_body());
    assert_eq!(
        b"0123456789".to_vec(),
        read_to_end(&mut first).await.unwrap()
    );
    assert_eq!(
        b"0123456789".to_vec(),
        read_to_end(&mut second).await.unwrap()
    );
    assert_eq!(1, upstream.requests().len(), "상위 서버에서 한 번만 받는다");
}

#[tokio::test]
async fn range_request_during_a_download_follows_it() {
    let dir = tempfile::tempdir().unwrap();
    let gate = Gate::default();
    let upstream = split_upstream(&gate, b"56789");
    let router = fixture_for(&upstream.url, dir.path()).router();
    let download = open(&router, get(PATH)).await;

    let tail = open(
        &router,
        with_header(Method::GET, PATH, header::RANGE, "bytes=7-"),
    )
    .await;
    let first_byte = send(
        &router,
        with_header(Method::GET, PATH, header::RANGE, "bytes=0-0"),
    )
    .await;

    assert_eq!(StatusCode::PARTIAL_CONTENT, first_byte.status);
    assert_eq!("bytes 0-0/10", first_byte.headers[header::CONTENT_RANGE]);
    assert_eq!(b"0".to_vec(), first_byte.body);
    assert_eq!(StatusCode::PARTIAL_CONTENT, tail.status());
    assert_eq!("bytes 7-9/10", tail.headers()[header::CONTENT_RANGE]);
    assert_eq!("3", tail.headers()[header::CONTENT_LENGTH]);
    gate.open();
    assert_eq!(
        b"789".to_vec(),
        read_to_end(&mut tail.into_body()).await.unwrap()
    );
    assert_eq!(
        b"0123456789".to_vec(),
        read_to_end(&mut download.into_body()).await.unwrap()
    );
    assert_eq!(1, upstream.requests().len());
}

#[tokio::test]
async fn range_probe_of_an_uncached_file_is_answered_by_upstream_without_caching() {
    let dir = tempfile::tempdir().unwrap();
    let upstream = FakeUpstream::start(|request, stream| {
        let reply: &[u8] = if request.contains("\r\nrange: bytes=0-0\r\n") {
            b"HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 0-0/10\r\nContent-Length: 1\r\nConnection: close\r\n\r\n0"
        } else {
            b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\nConnection: close\r\n\r\n0123456789"
        };
        stream.write_all(reply).unwrap();
    });
    let router = fixture_for(&upstream.url, dir.path()).router();

    let probe = send(
        &router,
        with_header(Method::GET, PATH, header::RANGE, "bytes=0-0"),
    )
    .await;

    assert_eq!(StatusCode::PARTIAL_CONTENT, probe.status);
    assert_eq!("bytes 0-0/10", probe.headers[header::CONTENT_RANGE]);
    assert_eq!("1", probe.headers[header::CONTENT_LENGTH]);
    assert_eq!(
        "application/octet-stream",
        probe.headers[header::CONTENT_TYPE]
    );
    assert_eq!(b"0".to_vec(), probe.body);
    assert_eq!(1, upstream.requests().len());
    assert!(
        files_under(dir.path()).is_empty(),
        "정보 요청은 캐시를 채우지 않는다"
    );

    let full = send(&router, get(PATH)).await;
    assert_eq!(b"0123456789".to_vec(), full.body);
    assert_eq!(2, upstream.requests().len());
}

#[tokio::test]
async fn head_of_an_uncached_file_asks_upstream_with_head_without_caching() {
    let dir = tempfile::tempdir().unwrap();
    let upstream = FakeUpstream::start(|_, stream| {
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\nConnection: close\r\n\r\n")
            .unwrap();
    });
    let router = fixture_for(&upstream.url, dir.path()).router();

    let reply = send(&router, head(PATH)).await;

    assert_eq!(StatusCode::OK, reply.status);
    assert_eq!("10", reply.headers[header::CONTENT_LENGTH]);
    assert!(reply.body.is_empty());
    let requests = upstream.requests();
    assert_eq!(1, requests.len());
    assert!(requests[0].starts_with("head "), "{}", requests[0]);
    assert!(files_under(dir.path()).is_empty());
}

#[tokio::test]
async fn probe_failures_before_any_byte_keep_their_status() {
    let setup = setup();
    setup.server.mock(|when, then| {
        when.method(GET).path(UPSTREAM_PATH);
        then.status(404);
    });
    let router = setup.fixture.router();
    let dir = tempfile::tempdir().unwrap();
    // 비워 둔 임시 포트는 함께 도는 다른 시험의 가짜 서버가 다시 잡을 수 있다. 1번은 듣는 쪽이 없다.
    let unreachable = fixture_for(UNREACHABLE_UPSTREAM, dir.path()).router();

    let missing = send(
        &router,
        with_header(Method::GET, PATH, header::RANGE, "bytes=0-0"),
    )
    .await;
    let down = send(
        &unreachable,
        with_header(Method::GET, PATH, header::RANGE, "bytes=0-0"),
    )
    .await;

    assert_eq!(StatusCode::NOT_FOUND, missing.status);
    assert_eq!(StatusCode::BAD_GATEWAY, down.status);
    assert!(down.json()["message"].is_string());
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
    let probe = send(
        &setup.fixture.router(),
        with_header(
            Method::GET,
            "/api/local-models/evil/model/resolve/main/config.json",
            header::RANGE,
            "bytes=0-0",
        ),
    )
    .await;

    assert_eq!(StatusCode::FORBIDDEN, reply.status);
    assert_eq!(StatusCode::FORBIDDEN, probe.status);
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
