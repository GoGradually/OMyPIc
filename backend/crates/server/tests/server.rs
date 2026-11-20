//! 실제 `start`로 띄운 서버에 HTTP로 요청하는 시험. 모든 시험은 다중 스레드 런타임에서 돈다
//! (블로킹 HTTP 클라이언트를 비동기 작업자에서 만들거나 놓으면 패닉하는지도 함께 본다).

use std::net::{Ipv4Addr, SocketAddr};
use std::path::Path;
use std::time::{Duration, Instant};

use omypic_infrastructure::config::{AppConfig, RagConfig};
use omypic_server::{start, RunningServer};
use reqwest::multipart::{Form, Part};
use reqwest::StatusCode;
use serde_json::{json, Value};

fn ephemeral() -> SocketAddr {
    SocketAddr::from((Ipv4Addr::LOCALHOST, 0))
}

fn hash_config(data_dir: &Path) -> AppConfig {
    let defaults = AppConfig::default();
    AppConfig {
        data_dir: data_dir.to_string_lossy().into_owned(),
        rag: RagConfig {
            provider: "hash".to_string(),
            ..defaults.rag.clone()
        },
        ..defaults
    }
}

async fn start_hash(data_dir: &Path) -> RunningServer {
    start(hash_config(data_dir), ephemeral()).await.unwrap()
}

fn url(server: &RunningServer, path: &str) -> String {
    format!("http://{}{}", server.addr, path)
}

async fn get_json(server: &RunningServer, path: &str) -> (StatusCode, Value) {
    let response = reqwest::get(url(server, path)).await.unwrap();
    let status = response.status();
    let body = response.text().await.unwrap();
    let value = if body.is_empty() {
        Value::Null
    } else {
        serde_json::from_str(&body).unwrap()
    };
    (status, value)
}

async fn send_json(
    server: &RunningServer,
    method: reqwest::Method,
    path: &str,
    body: Value,
) -> (StatusCode, Value) {
    let response = reqwest::Client::new()
        .request(method, url(server, path))
        .json(&body)
        .send()
        .await
        .unwrap();
    let status = response.status();
    let text = response.text().await.unwrap();
    let value = if text.is_empty() {
        Value::Null
    } else {
        serde_json::from_str(&text).unwrap()
    };
    (status, value)
}

async fn create_group(server: &RunningServer, name: &str, tags: &[&str]) -> Value {
    let (status, group) = send_json(
        server,
        reqwest::Method::POST,
        "/api/question-groups",
        json!({"name": name, "tags": tags}),
    )
    .await;
    assert_eq!(StatusCode::OK, status, "{group}");
    group
}

async fn upload_rulebook(server: &RunningServer, filename: &str, text: &str) -> Value {
    let form = Form::new()
        .part(
            "file",
            Part::bytes(text.as_bytes().to_vec()).file_name(filename.to_string()),
        )
        .text("scope", "MAIN");
    let response = reqwest::Client::new()
        .post(url(server, "/api/rulebooks"))
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(StatusCode::OK, response.status());
    response.json().await.unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn server_starts_on_ephemeral_port_and_answers_ping() {
    let dir = tempfile::tempdir().unwrap();
    let server = start_hash(dir.path()).await;
    assert_ne!(0, server.addr.port());
    assert!(server.addr.ip().is_loopback());

    let (status, health) = get_json(&server, "/actuator/health").await;
    assert_eq!(StatusCode::OK, status);
    assert_eq!(json!({"status": "UP"}), health);
    // Java 백엔드에는 `/api/ping`이 없다(시험용 컨트롤러뿐). 같은 API를 지키므로 404다.
    let (status, _) = get_json(&server, "/api/ping").await;
    assert_eq!(StatusCode::NOT_FOUND, status);
    let (status, models) = get_json(&server, "/api/meta/models").await;
    assert_eq!(StatusCode::OK, status);
    assert_eq!("gpt-5-nano", models["defaultFeedbackModel"]);

    assert!(dir.path().join("omypic.db").is_file());
    server.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn start_then_shutdown_does_not_panic() {
    let dir = tempfile::tempdir().unwrap();
    let server = start_hash(&dir.path().join("nested").join("data")).await;
    let addr = server.addr;
    server.shutdown().await.unwrap();
    assert!(reqwest::get(format!("http://{addr}/actuator/health"))
        .await
        .is_err());
}

#[tokio::test(flavor = "multi_thread")]
async fn question_group_crud_round_trip_over_http() {
    let dir = tempfile::tempdir().unwrap();
    let server = start_hash(dir.path()).await;

    let group = create_group(&server, "여행", &["travel", "habit"]).await;
    let group_id = group["id"].as_str().unwrap().to_string();
    assert_eq!(json!(["habit", "travel"]), group["tags"]);

    let (status, updated) = send_json(
        &server,
        reqwest::Method::POST,
        &format!("/api/question-groups/{group_id}/items"),
        json!({"text": "Tell me about your last trip.", "questionType": "habit"}),
    )
    .await;
    assert_eq!(StatusCode::OK, status);
    assert_eq!(1, updated["questions"].as_array().unwrap().len());
    let item_id = updated["questions"][0]["id"].as_str().unwrap().to_string();

    let (status, list) = get_json(&server, "/api/question-groups").await;
    assert_eq!(StatusCode::OK, status);
    assert_eq!(1, list.as_array().unwrap().len());
    assert_eq!("여행", list[0]["name"]);
    assert_eq!(
        "Tell me about your last trip.",
        list[0]["questions"][0]["text"]
    );

    let (status, after_item_delete) = send_json(
        &server,
        reqwest::Method::DELETE,
        &format!("/api/question-groups/{group_id}/items/{item_id}"),
        Value::Null,
    )
    .await;
    assert_eq!(StatusCode::OK, status);
    assert_eq!(json!([]), after_item_delete["questions"]);

    let (status, _) = send_json(
        &server,
        reqwest::Method::DELETE,
        &format!("/api/question-groups/{group_id}"),
        Value::Null,
    )
    .await;
    assert_eq!(StatusCode::OK, status);
    let (_, list) = get_json(&server, "/api/question-groups").await;
    assert_eq!(json!([]), list);

    server.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn data_survives_restart() {
    let dir = tempfile::tempdir().unwrap();
    let server = start_hash(dir.path()).await;
    let group = create_group(&server, "Home", &["home"]).await;
    upload_rulebook(&server, "규칙.md", "# 규칙\n과거 시제를 쓴다.").await;
    server.shutdown().await.unwrap();

    let server = start_hash(dir.path()).await;
    let (_, list) = get_json(&server, "/api/question-groups").await;
    assert_eq!(1, list.as_array().unwrap().len());
    assert_eq!(group["id"], list[0]["id"]);
    assert_eq!("Home", list[0]["name"]);
    let (_, rulebooks) = get_json(&server, "/api/rulebooks").await;
    assert_eq!("규칙.md", rulebooks[0]["filename"]);
    server.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn export_import_over_http_round_trips() {
    let source_dir = tempfile::tempdir().unwrap();
    let source = start_hash(source_dir.path()).await;
    let group = create_group(&source, "Travel", &["travel"]).await;
    send_json(
        &source,
        reqwest::Method::POST,
        &format!(
            "/api/question-groups/{}/items",
            group["id"].as_str().unwrap()
        ),
        json!({"text": "Where did you go?", "questionType": null}),
    )
    .await;
    upload_rulebook(&source, "문법 규칙.md", "# Rules\nUse past tense.").await;

    let response = reqwest::get(url(&source, "/api/data-transfer/export"))
        .await
        .unwrap();
    assert_eq!(StatusCode::OK, response.status());
    assert_eq!("application/zip", response.headers()["content-type"]);
    let zip = response.bytes().await.unwrap().to_vec();
    let (_, source_groups) = get_json(&source, "/api/question-groups").await;
    let (_, source_rulebooks) = get_json(&source, "/api/rulebooks").await;
    source.shutdown().await.unwrap();

    let target_dir = tempfile::tempdir().unwrap();
    let target = start_hash(target_dir.path()).await;
    let form = Form::new().part(
        "file",
        Part::bytes(zip).file_name("omypic-backup.zip".to_string()),
    );
    let response = reqwest::Client::new()
        .post(url(&target, "/api/data-transfer/import"))
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(StatusCode::OK, response.status());
    let summary: Value = response.json().await.unwrap();
    assert_eq!(1, summary["questionGroupCount"]);
    assert_eq!(1, summary["rulebookCount"]);

    let (_, target_groups) = get_json(&target, "/api/question-groups").await;
    assert_eq!(source_groups, target_groups);
    let (_, target_rulebooks) = get_json(&target, "/api/rulebooks").await;
    let strip_path = |value: &Value| {
        let mut list = value.clone();
        for item in list.as_array_mut().unwrap() {
            item.as_object_mut().unwrap().remove("path");
        }
        list
    };
    // 파일 경로는 새 데이터 폴더 기준이므로 그것만 빼고 같다.
    assert_eq!(strip_path(&source_rulebooks), strip_path(&target_rulebooks));
    let path = target_rulebooks[0]["path"].as_str().unwrap();
    assert!(Path::new(path).starts_with(target_dir.path()), "{path}");
    assert!(Path::new(path).is_file());
    target.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn metrics_endpoint_reads_what_use_cases_record() {
    let dir = tempfile::tempdir().unwrap();
    let server = start_hash(dir.path()).await;
    let (status, _) = get_json(&server, "/actuator/metrics/rulebook.upload.latency").await;
    assert_eq!(StatusCode::NOT_FOUND, status);

    upload_rulebook(&server, "rules.md", "Use past tense.").await;

    let (status, metric) = get_json(&server, "/actuator/metrics/rulebook.upload.latency").await;
    assert_eq!(StatusCode::OK, status);
    assert_eq!("COUNT", metric["measurements"][0]["statistic"]);
    assert_eq!(1.0, metric["measurements"][0]["value"]);
    server.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn fasttext_model_is_not_loaded_at_startup() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = hash_config(dir.path());
    config.rag.provider = "fasttext".to_string();
    // 닿지 않는 주소: 시작할 때 내려받으려 하면 실패하거나 오래 걸린다.
    config.rag.download_url = "http://127.0.0.1:9/cc.ko.300.vec.gz".to_string();
    let started = Instant::now();
    let server = start(config, ephemeral()).await.unwrap();
    assert!(started.elapsed() < Duration::from_secs(5));
    let (status, _) = get_json(&server, "/api/question-groups").await;
    assert_eq!(StatusCode::OK, status);
    assert!(!dir.path().join("models").exists());
    server.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn unknown_rag_provider_fails_to_start() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = hash_config(dir.path());
    config.rag.provider = "lucene".to_string();
    let error = start(config, ephemeral()).await.err().unwrap();
    assert!(
        error
            .to_string()
            .contains("Unsupported rag provider 'lucene'"),
        "{error}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn bind_failure_names_the_address() {
    let dir = tempfile::tempdir().unwrap();
    let first = start_hash(dir.path()).await;
    let error = start(hash_config(dir.path()), first.addr)
        .await
        .err()
        .unwrap();
    let expected = format!("Failed to bind {}: ", first.addr);
    assert!(error.to_string().starts_with(&expected), "{error}");
    first.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn shutdown_ends_open_event_streams() {
    let dir = tempfile::tempdir().unwrap();
    let server = start_hash(dir.path()).await;
    let (status, opened) = send_json(
        &server,
        reqwest::Method::POST,
        "/api/voice/sessions",
        json!({"sessionId": "s-1", "aiEnabled": false}),
    )
    .await;
    assert_eq!(StatusCode::OK, status, "{opened}");
    let voice_session_id = opened["voiceSessionId"].as_str().unwrap().to_string();
    let response = reqwest::get(url(
        &server,
        &format!("/api/voice/sessions/{voice_session_id}/events"),
    ))
    .await
    .unwrap();
    assert_eq!(StatusCode::OK, response.status());
    let reader = tokio::spawn(async move { response.text().await });

    let started = Instant::now();
    server.shutdown().await.unwrap();
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "shutdown waited {:?}",
        started.elapsed()
    );
    let body = tokio::time::timeout(Duration::from_secs(5), reader)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(body.contains("event: session.ready"), "{body}");
}

#[tokio::test(flavor = "multi_thread")]
async fn abandoned_voice_session_is_stopped_and_removed_without_further_requests() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = hash_config(dir.path());
    config.voice.idle_timeout_ms = 1_000;
    config.voice.recovery_retention_ms = 1_000;
    let server = start(config, ephemeral()).await.unwrap();
    let (status, opened) = send_json(
        &server,
        reqwest::Method::POST,
        "/api/voice/sessions",
        json!({"sessionId": "s-1", "aiEnabled": false}),
    )
    .await;
    assert_eq!(StatusCode::OK, status, "{opened}");
    let voice_session_id = opened["voiceSessionId"].as_str().unwrap().to_string();

    // No request at all: only the server's own sweep can stop the session and then drop it.
    tokio::time::sleep(Duration::from_millis(4_500)).await;

    let (status, body) = get_json(
        &server,
        &format!("/api/voice/sessions/{voice_session_id}/recovery"),
    )
    .await;
    assert_eq!(StatusCode::BAD_REQUEST, status, "{body}");
    assert_eq!(
        format!("Unknown voice session: {voice_session_id}"),
        body["message"]
    );
    server.shutdown().await.unwrap();
}

fn file_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

async fn import_zip(server: &RunningServer, zip: Vec<u8>) -> (StatusCode, Value) {
    let form = Form::new().part(
        "file",
        Part::bytes(zip).file_name("omypic-backup.zip".to_string()),
    );
    let response = reqwest::Client::new()
        .post(url(server, "/api/data-transfer/import"))
        .multipart(form)
        .send()
        .await
        .unwrap();
    let status = response.status();
    (status, response.json().await.unwrap())
}

#[tokio::test(flavor = "multi_thread")]
async fn import_that_fails_on_unavailable_embedding_model_keeps_existing_data() {
    let dir = tempfile::tempdir().unwrap();
    let source = start_hash(dir.path()).await;
    create_group(&source, "Exported", &["travel"]).await;
    upload_rulebook(&source, "rules.md", "# Rules\nUse past tense.").await;
    let zip = reqwest::get(url(&source, "/api/data-transfer/export"))
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap()
        .to_vec();
    create_group(&source, "Local only", &["home"]).await;
    let (_, groups_before) = get_json(&source, "/api/question-groups").await;
    let (_, rulebooks_before) = get_json(&source, "/api/rulebooks").await;
    source.shutdown().await.unwrap();
    let files_before = file_names(&dir.path().join("rulebooks"));
    assert_eq!(1, files_before.len());

    // 같은 데이터 폴더를, 모델을 구할 수 없는 fastText 설정으로 다시 연다.
    let mut config = hash_config(dir.path());
    config.rag.provider = "fasttext".to_string();
    config.rag.download_url = "http://127.0.0.1:9/cc.ko.300.vec.gz".to_string();
    config.rag.download_retry_max = 0;
    let server = start(config, ephemeral()).await.unwrap();

    let (status, body) = import_zip(&server, zip).await;

    assert_eq!(StatusCode::BAD_REQUEST, status, "{body}");
    let (_, groups_after) = get_json(&server, "/api/question-groups").await;
    assert_eq!(groups_before, groups_after);
    let (_, rulebooks_after) = get_json(&server, "/api/rulebooks").await;
    assert_eq!(rulebooks_before, rulebooks_after);
    assert_eq!(files_before, file_names(&dir.path().join("rulebooks")));
    server.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn configured_origins_are_allowed_and_others_are_forbidden() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = hash_config(dir.path());
    config.allowed_origins = vec!["http://localhost:3000".to_string()];
    let server = start(config, ephemeral()).await.unwrap();
    let client = reqwest::Client::new();
    let get_with_origin = |origin: &'static str| {
        client
            .get(url(&server, "/api/question-groups"))
            .header("Origin", origin)
            .send()
    };

    let allowed = get_with_origin("http://localhost:3000").await.unwrap();
    assert_eq!(StatusCode::OK, allowed.status());
    assert_eq!(
        "http://localhost:3000",
        allowed.headers()["access-control-allow-origin"]
    );
    // 목록을 설정하면 기본 목록(개발 서버 origin 포함)을 대신한다.
    let replaced = get_with_origin("http://localhost:5173").await.unwrap();
    assert_eq!(StatusCode::FORBIDDEN, replaced.status());
    let evil = get_with_origin("https://evil.example").await.unwrap();
    assert_eq!(StatusCode::FORBIDDEN, evil.status());
    assert_eq!(
        json!({"message": "Origin not allowed"}),
        evil.json::<Value>().await.unwrap()
    );
    server.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn shutdown_returns_within_the_grace_period_while_a_request_is_blocked() {
    use std::sync::{Arc, Mutex};

    // 연결을 받고 아무것도 보내지 않는 모델 서버. 시험 끝에 연결을 닫아 멈춘 다운로드를 끝낸다.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let model_addr = listener.local_addr().unwrap();
    let held: Arc<Mutex<Vec<std::net::TcpStream>>> = Arc::new(Mutex::new(Vec::new()));
    let held_by_server = held.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            held_by_server.lock().unwrap().push(stream);
        }
    });

    let dir = tempfile::tempdir().unwrap();
    let mut config = hash_config(dir.path());
    config.rag.provider = "fasttext".to_string();
    config.rag.download_url = format!("http://{model_addr}/cc.ko.300.vec.gz");
    config.rag.download_retry_max = 0;
    let server = start(config, ephemeral()).await.unwrap();
    let upload_url = url(&server, "/api/rulebooks");
    let upload = tokio::spawn(async move {
        let form = Form::new().part(
            "file",
            Part::bytes(b"# rules".to_vec()).file_name("rules.md".to_string()),
        );
        reqwest::Client::new()
            .post(upload_url)
            .multipart(form)
            .send()
            .await
    });
    let waiting = Instant::now();
    while held.lock().unwrap().is_empty() {
        assert!(
            waiting.elapsed() < Duration::from_secs(10),
            "upload never blocked"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let started = Instant::now();
    server.shutdown().await.unwrap();
    let took = started.elapsed();
    // 멈춘 다운로드를 끝내서 시험 런타임이 블로킹 작업을 기다리지 않게 한다.
    held.lock().unwrap().clear();

    assert!(
        took < omypic_server::SHUTDOWN_GRACE + Duration::from_secs(2),
        "shutdown took {took:?}"
    );
    assert_eq!(Duration::from_secs(5), omypic_server::SHUTDOWN_GRACE);
    let _ = upload.await;
}
