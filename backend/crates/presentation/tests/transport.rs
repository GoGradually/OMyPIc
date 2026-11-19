//! HTTP 계층 자체의 동작: 본문 제한, multipart 파일 이름, SSE 전달과 해제, 오류 변환, 지표.

mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::Body;
use axum::http::{header, Method, StatusCode};
use axum::response::IntoResponse;
use common::*;
use omypic_application::question::port::QuestionGroupPort;
use omypic_application::shared::port::MetricsPort;
use omypic_application::voice::model::VoiceEventSink;
use omypic_domain::error::{AppError, AppResult};
use omypic_domain::question::{QuestionGroupAggregate, QuestionGroupId};
use omypic_presentation::sse::{open_event_stream, SinkRegistry};
use serde_json::json;
use tower::ServiceExt;

#[tokio::test]
async fn audio_chunk_accepts_8mb_json_body() {
    let fixture = Fixture::new();
    fixture.add_group(group(
        "g-1",
        "travel",
        &["travel"],
        vec![("q-1", "question", "OPEN")],
    ));
    let router = fixture.router();
    let mode = send(
        &router,
        json_request(
            Method::PUT,
            "/api/modes",
            r#"{"sessionId":"s1","mode":"IMMEDIATE","selectedGroupTags":["travel"]}"#,
        ),
    )
    .await;
    assert_eq!(StatusCode::NO_CONTENT, mode.status);
    let opened = send(&router, {
        let mut request = post_json("/api/voice/sessions", r#"{"sessionId":"s1"}"#);
        request
            .headers_mut()
            .insert("X-API-Key", "key".parse().unwrap());
        request
    })
    .await
    .json();
    let id = opened["voiceSessionId"].as_str().unwrap();
    let mut stream = router
        .clone()
        .oneshot(get(&format!("/api/voice/sessions/{id}/events")))
        .await
        .unwrap()
        .into_body();
    read_sse_events(&mut stream, 2, Duration::from_secs(2)).await;

    let chunk = "A".repeat(8 * 1024 * 1024);
    let body = format!(r#"{{"pcm16Base64":"{chunk}","sampleRate":16000,"sequence":1}}"#);
    assert!(body.len() > 8 * 1024 * 1024);
    let reply = send(
        &router,
        post_json(&format!("/api/voice/sessions/{id}/audio-chunks"), &body),
    )
    .await;

    assert_ne!(StatusCode::PAYLOAD_TOO_LARGE, reply.status);
    assert_eq!(StatusCode::NO_CONTENT, reply.status, "{}", reply.text());
}

#[tokio::test]
async fn json_body_over_64mb_is_rejected_with_413() {
    let fixture = Fixture::new();
    let padding = "a".repeat(64 * 1024 * 1024 + 1);
    let body = format!(r#"{{"name":"{padding}"}}"#);
    let reply = send(&fixture.router(), post_json("/api/question-groups", &body)).await;

    assert_eq!(StatusCode::PAYLOAD_TOO_LARGE, reply.status);
    assert_eq!("Request body is too large", reply.json()["message"]);
}

#[tokio::test]
async fn rulebook_upload_accepts_korean_filename() {
    let fixture = Fixture::new();
    let reply = send(
        &fixture.router(),
        multipart(
            Method::POST,
            "/api/rulebooks",
            &[Part::File {
                name: "file",
                filename: "오픽 규칙.md",
                bytes: "# 규칙".as_bytes(),
            }],
        ),
    )
    .await;

    assert_eq!(StatusCode::OK, reply.status, "{}", reply.text());
    assert_eq!("오픽 규칙.md", reply.json()["filename"]);
    assert_eq!("MAIN", reply.json()["scope"]);
}

type Registration = (String, Option<i64>, Arc<dyn VoiceEventSink>);

#[derive(Default)]
struct SpyRegistry {
    registered: Mutex<Vec<Registration>>,
    unregistered: Mutex<Vec<(String, Arc<dyn VoiceEventSink>)>>,
}

impl SinkRegistry for SpyRegistry {
    fn register(
        &self,
        voice_session_id: &str,
        sink: Arc<dyn VoiceEventSink>,
        since_event_id: Option<i64>,
    ) -> AppResult<()> {
        if voice_session_id == "unknown" {
            return Err(AppError::InvalidArgument(
                "Unknown voice session: unknown".to_string(),
            ));
        }
        self.registered
            .lock()
            .unwrap()
            .push((voice_session_id.to_string(), since_event_id, sink));
        Ok(())
    }

    fn unregister(&self, voice_session_id: &str, sink: &Arc<dyn VoiceEventSink>) {
        self.unregistered
            .lock()
            .unwrap()
            .push((voice_session_id.to_string(), Arc::clone(sink)));
    }
}

async fn open_spy_stream(spy: &Arc<SpyRegistry>, timeout: Duration) -> Body {
    let sse = open_event_stream(spy.clone(), "v1".to_string(), Some(7), timeout)
        .await
        .unwrap();
    sse.into_response().into_body()
}

async fn read_raw(body: &mut Body, timeout: Duration) -> String {
    use http_body_util::BodyExt;
    let mut text = String::new();
    while let Ok(Some(Ok(frame))) = tokio::time::timeout(timeout, body.frame()).await {
        if let Some(data) = frame.data_ref() {
            text.push_str(&String::from_utf8_lossy(data));
        }
    }
    text
}

#[tokio::test]
async fn sse_stream_delivers_events_in_order() {
    let spy = Arc::new(SpyRegistry::default());
    let mut body = open_spy_stream(&spy, Duration::from_secs(60)).await;
    let sink = {
        let registered = spy.registered.lock().unwrap();
        assert_eq!("v1", registered[0].0);
        assert_eq!(Some(7), registered[0].1);
        registered[0].2.clone()
    };

    assert!(sink.send("session.ready", json!({"eventId": 1, "sessionId": "s1"})));
    assert!(sink.send("question.prompt", json!({"eventId": 2, "turnId": 1})));
    assert!(sink.send("stt.final", json!({"eventId": 3, "text": "답변"})));

    let raw = read_raw(&mut body, Duration::from_millis(200)).await;
    let lines: Vec<&str> = raw.lines().filter(|line| !line.is_empty()).collect();
    assert_eq!(
        vec![
            "event: session.ready",
            r#"data: {"eventId":1,"sessionId":"s1"}"#,
            "event: question.prompt",
            r#"data: {"eventId":2,"turnId":1}"#,
            "event: stt.final",
            r#"data: {"eventId":3,"text":"답변"}"#,
        ],
        lines
    );
    assert!(!raw.contains("\nid:") && !raw.starts_with("id:"));
}

#[tokio::test]
async fn sse_response_headers_mark_an_event_stream() {
    let spy = Arc::new(SpyRegistry::default());
    let response = open_event_stream(spy, "v1".to_string(), None, Duration::from_secs(60))
        .await
        .unwrap()
        .into_response();

    assert_eq!(StatusCode::OK, response.status());
    assert_eq!(
        "text/event-stream",
        response.headers()[header::CONTENT_TYPE]
    );
    assert_eq!("no-cache", response.headers()[header::CACHE_CONTROL]);
}

#[tokio::test]
async fn sse_disconnect_unregisters_sink() {
    let spy = Arc::new(SpyRegistry::default());
    let body = open_spy_stream(&spy, Duration::from_secs(60)).await;
    let sink = spy.registered.lock().unwrap()[0].2.clone();
    assert!(sink.send("session.ready", json!({"eventId": 1})));

    drop(body);

    wait_until(|| !spy.unregistered.lock().unwrap().is_empty()).await;
    let unregistered = spy.unregistered.lock().unwrap();
    assert_eq!("v1", unregistered[0].0);
    assert!(
        Arc::ptr_eq(&sink, &unregistered[0].1),
        "must unregister the same Arc it registered"
    );
    drop(unregistered);
    assert!(!sink.send("session.ready", json!({"eventId": 2})));
}

#[tokio::test]
async fn sse_stream_ends_and_unregisters_after_timeout() {
    let spy = Arc::new(SpyRegistry::default());
    let mut body = open_spy_stream(&spy, Duration::from_millis(100)).await;
    let sink = spy.registered.lock().unwrap()[0].2.clone();
    assert!(sink.send("session.ready", json!({"eventId": 1})));

    let raw = read_raw(&mut body, Duration::from_secs(5)).await;

    assert!(raw.contains("event: session.ready"));
    wait_until(|| !spy.unregistered.lock().unwrap().is_empty()).await;
    assert!(!sink.send("late", json!({})));
}

#[tokio::test]
async fn sse_registration_failure_is_reported_as_bad_request() {
    let spy = Arc::new(SpyRegistry::default());
    let Err(error) = open_event_stream(
        spy.clone(),
        "unknown".to_string(),
        None,
        Duration::from_secs(1),
    )
    .await
    else {
        panic!("registration must fail");
    };
    let response = error.into_response();

    assert_eq!(StatusCode::BAD_REQUEST, response.status());
    assert!(spy.registered.lock().unwrap().is_empty());
}

async fn wait_until(condition: impl Fn() -> bool) {
    for _ in 0..200 {
        if condition() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("condition not met in time");
}

#[tokio::test]
async fn unknown_route_returns_404() {
    let fixture = Fixture::new();
    let reply = send(&fixture.router(), get("/api/nope")).await;

    assert_eq!(StatusCode::NOT_FOUND, reply.status);
    assert_eq!("Not found", reply.json()["message"]);
}

#[tokio::test]
async fn malformed_json_returns_400_with_message() {
    let fixture = Fixture::new();
    let router = fixture.router();

    for (uri, body) in [
        ("/api/keys/verify", "{not json"),
        ("/api/keys/verify", ""),
        ("/api/question-groups", r#"{"name": 5}"#),
        ("/api/voice/sessions/v1/stop", "{broken"),
        ("/api/voice/sessions/v1/answers", r#"{"sequence":"x"}"#),
    ] {
        let reply = send(&router, post_json(uri, body)).await;
        assert_eq!(StatusCode::BAD_REQUEST, reply.status, "{uri} {body}");
        assert_eq!(
            "Malformed JSON request body",
            reply.json()["message"],
            "{uri}"
        );
    }
}

#[tokio::test]
async fn malformed_json_message_does_not_echo_the_body() {
    let fixture = Fixture::new();
    let reply = send(
        &fixture.router(),
        post_json(
            "/api/keys/verify",
            r#"{"provider":"openai","apiKey":123456}"#,
        ),
    )
    .await;

    assert_eq!(StatusCode::BAD_REQUEST, reply.status);
    assert!(!reply.text().contains("123456"));
}

#[tokio::test]
async fn health_reports_up() {
    let fixture = Fixture::new();
    let reply = send(&fixture.router(), get("/actuator/health")).await;

    assert_eq!(StatusCode::OK, reply.status);
    assert_eq!(json!({"status": "UP"}), reply.json());
}

#[tokio::test]
async fn metrics_endpoint_reports_counter() {
    let fixture = Fixture::new();
    fixture.metrics.increment_stt_request();
    fixture.metrics.increment_stt_request();
    let router = fixture.router();

    let names = send(&router, get("/actuator/metrics")).await;
    assert_eq!(StatusCode::OK, names.status);
    assert_eq!(json!({"names": ["stt.requests"]}), names.json());

    let metric = send(&router, get("/actuator/metrics/stt.requests")).await;
    assert_eq!(StatusCode::OK, metric.status);
    assert_eq!(
        json!({
            "name": "stt.requests",
            "measurements": [{"statistic": "COUNT", "value": 2.0}]
        }),
        metric.json()
    );
}

#[tokio::test]
async fn unknown_metric_returns_404() {
    let fixture = Fixture::new();
    let reply = send(&fixture.router(), get("/actuator/metrics/no.such.metric")).await;

    assert_eq!(StatusCode::NOT_FOUND, reply.status);
}

/// 유스케이스가 패닉하는 저장소.
struct PanickingQuestionPort;

impl QuestionGroupPort for PanickingQuestionPort {
    fn find_all(&self) -> AppResult<Vec<QuestionGroupAggregate>> {
        panic!("port exploded");
    }
    fn find_all_by_id(&self, _: &[QuestionGroupId]) -> AppResult<Vec<QuestionGroupAggregate>> {
        panic!("port exploded");
    }
    fn find_by_id(&self, _: &QuestionGroupId) -> AppResult<Option<QuestionGroupAggregate>> {
        panic!("port exploded");
    }
    fn save(&self, _: QuestionGroupAggregate) -> AppResult<QuestionGroupAggregate> {
        panic!("port exploded");
    }
    fn delete_by_id(&self, _: &QuestionGroupId) -> AppResult<()> {
        panic!("port exploded");
    }
    fn delete_all(&self) -> AppResult<()> {
        panic!("port exploded");
    }
}

#[tokio::test]
async fn panicking_use_case_returns_500_and_server_keeps_answering() {
    let fixture = Fixture::with_question_port(Arc::new(PanickingQuestionPort));
    let router = fixture.router();

    let reply = send(&router, get("/api/question-groups")).await;
    assert_eq!(StatusCode::INTERNAL_SERVER_ERROR, reply.status);
    assert_eq!(json!({"message": "Internal server error"}), reply.json());

    let again = send(&router, get("/api/question-groups")).await;
    assert_eq!(StatusCode::INTERNAL_SERVER_ERROR, again.status);
    let health = send(&router, get("/actuator/health")).await;
    assert_eq!(StatusCode::OK, health.status);
    let notes = send(&router, get("/api/wrongnotes")).await;
    assert_eq!(StatusCode::OK, notes.status);
}

/// 비동기 워커에서 불렸다면 `block_on`이 패닉한다(`reqwest::blocking`과 같은 제약).
struct BlockingAwarePort;

impl QuestionGroupPort for BlockingAwarePort {
    fn find_all(&self) -> AppResult<Vec<QuestionGroupAggregate>> {
        tokio::runtime::Handle::current().block_on(async {});
        Ok(vec![])
    }
    fn find_all_by_id(&self, _: &[QuestionGroupId]) -> AppResult<Vec<QuestionGroupAggregate>> {
        Ok(vec![])
    }
    fn find_by_id(&self, _: &QuestionGroupId) -> AppResult<Option<QuestionGroupAggregate>> {
        Ok(None)
    }
    fn save(&self, group: QuestionGroupAggregate) -> AppResult<QuestionGroupAggregate> {
        Ok(group)
    }
    fn delete_by_id(&self, _: &QuestionGroupId) -> AppResult<()> {
        Ok(())
    }
    fn delete_all(&self) -> AppResult<()> {
        Ok(())
    }
}

#[tokio::test]
async fn use_cases_run_off_the_async_workers() {
    let fixture = Fixture::with_question_port(Arc::new(BlockingAwarePort));
    let reply = send(&fixture.router(), get("/api/question-groups")).await;

    assert_eq!(StatusCode::OK, reply.status);
    assert_eq!(json!([]), reply.json());
}
