//! Java 컨트롤러 테스트(`*ControllerTest`, `WebConfigTest`)를 옮긴 것이다. Mockito mock 대신
//! `test_support` 가짜 위에 실제 유스케이스를 조립하고, 컨트롤러가 유스케이스에 넘긴 값은
//! 응답과 가짜 저장소의 상태로 확인한다.

mod common;

use std::time::Duration;

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use chrono::{TimeZone, Utc};
use common::*;
use omypic_domain::rulebook::{Rulebook, RulebookId, RulebookScope};
use omypic_domain::session::ModeType;
use omypic_domain::wrongnote::{WrongNote, WrongNoteId};
use serde_json::json;
use tower::ServiceExt;

// ---- ApiKeyControllerTest

#[tokio::test]
async fn verify_returns_result_envelope() {
    let fixture = Fixture::new();
    let reply = send(
        &fixture.router(),
        post_json(
            "/api/keys/verify",
            r#"{"provider":"openai","apiKey":"sk-test","model":"gpt-4o-mini"}"#,
        ),
    )
    .await;

    assert_eq!(StatusCode::OK, reply.status);
    let body = reply.json();
    assert_eq!(json!(true), body["valid"]);
    assert_eq!("openai", body["provider"]);
    assert!(body["checkedAt"].as_str().unwrap().ends_with('Z'));
    assert!(body["message"].is_string());
}

#[tokio::test]
async fn verify_returns_bad_request_when_missing_fields() {
    let fixture = Fixture::new();
    let reply = send(
        &fixture.router(),
        post_json("/api/keys/verify", r#"{"provider":"openai"}"#),
    )
    .await;

    assert_eq!(StatusCode::BAD_REQUEST, reply.status);
    assert_eq!("apiKey must not be blank", reply.json()["message"]);
}

#[tokio::test]
async fn verify_returns_bad_request_when_blank_provider() {
    let fixture = Fixture::new();
    let reply = send(
        &fixture.router(),
        post_json("/api/keys/verify", r#"{"provider":"  ","apiKey":"k"}"#),
    )
    .await;

    assert_eq!(StatusCode::BAD_REQUEST, reply.status);
    assert_eq!("provider must not be blank", reply.json()["message"]);
}

#[tokio::test]
async fn verify_returns_bad_request_when_feedback_model_unsupported() {
    let fixture = Fixture::new();
    let reply = send(
        &fixture.router(),
        post_json(
            "/api/keys/verify",
            r#"{"provider":"openai","apiKey":"sk-test","model":"gpt-5.2"}"#,
        ),
    )
    .await;

    assert_eq!(StatusCode::BAD_REQUEST, reply.status);
    assert!(reply.json()["message"]
        .as_str()
        .unwrap()
        .starts_with("Unsupported feedback model"));
}

// ---- QuestionControllerTest

#[tokio::test]
async fn question_list_returns_mapped_response() {
    let fixture = Fixture::new();
    fixture.add_group(group(
        "g1",
        "Travel",
        &["travel", "habit"],
        vec![("q1", "Q1", "habit")],
    ));
    let reply = send(&fixture.router(), get("/api/question-groups")).await;

    assert_eq!(StatusCode::OK, reply.status);
    let body = reply.json();
    assert_eq!("g1", body[0]["id"]);
    assert_eq!("Travel", body[0]["name"]);
    assert_eq!("habit", body[0]["tags"][0]);
    assert_eq!("travel", body[0]["tags"][1]);
    assert_eq!("q1", body[0]["questions"][0]["id"]);
    assert_eq!("Q1", body[0]["questions"][0]["text"]);
    assert_eq!("habit", body[0]["questions"][0]["questionType"]);
    assert_eq!("2026-02-01T00:00:00Z", body[0]["createdAt"]);
    assert_eq!("2026-02-01T00:00:00Z", body[0]["updatedAt"]);
}

#[tokio::test]
async fn create_passes_name_and_tags_to_use_case() {
    let fixture = Fixture::new();
    let reply = send(
        &fixture.router(),
        post_json(
            "/api/question-groups",
            r#"{"name":"New Group","tags":["travel"]}"#,
        ),
    )
    .await;

    assert_eq!(StatusCode::OK, reply.status);
    let body = reply.json();
    assert_eq!("New Group", body["name"]);
    assert_eq!(json!(["travel"]), body["tags"]);
    let saved = fixture.questions.groups();
    assert_eq!(1, saved.len());
    assert_eq!("New Group", saved[0].name());
    assert_eq!(&["travel".to_string()][..], saved[0].tags());
    assert_eq!(saved[0].id().value(), body["id"]);
}

#[tokio::test]
async fn add_question_passes_body_to_use_case() {
    let fixture = Fixture::new();
    fixture.add_group(group("g4", "Group", &["travel"], vec![]));
    let reply = send(
        &fixture.router(),
        post_json(
            "/api/question-groups/g4/items",
            r#"{"text":"new question","questionType":"compare"}"#,
        ),
    )
    .await;

    assert_eq!(StatusCode::OK, reply.status);
    let body = reply.json();
    assert_eq!("new question", body["questions"][0]["text"]);
    assert_eq!("compare", body["questions"][0]["questionType"]);
    let saved = fixture.questions.groups();
    assert_eq!("new question", saved[0].questions()[0].text());
    assert_eq!(Some("compare"), saved[0].questions()[0].question_type());
}

#[tokio::test]
async fn tag_stats_maps_use_case_response() {
    let fixture = Fixture::new();
    fixture.add_group(group(
        "g1",
        "A",
        &["habit", "travel"],
        vec![("q1", "Q1", "habit")],
    ));
    fixture.add_group(group("g2", "B", &["travel"], vec![("q2", "Q2", "habit")]));
    let reply = send(&fixture.router(), get("/api/question-groups/tags/stats")).await;

    assert_eq!(StatusCode::OK, reply.status);
    let body = reply.json();
    assert_eq!("habit", body[0]["tag"]);
    assert_eq!(1, body[0]["groupCount"]);
    assert_eq!(json!(true), body[0]["selectable"]);
    assert_eq!("travel", body[1]["tag"]);
    assert_eq!(2, body[1]["groupCount"]);
}

#[tokio::test]
async fn update_and_delete_question_group_routes() {
    let fixture = Fixture::new();
    fixture.add_group(group(
        "g1",
        "Travel",
        &["travel"],
        vec![("q1", "Q1", "habit"), ("q2", "Q2", "habit")],
    ));
    let router = fixture.router();

    let renamed = send(
        &router,
        json_request(
            Method::PUT,
            "/api/question-groups/g1",
            r#"{"name":"Trip","tags":["trip"]}"#,
        ),
    )
    .await;
    assert_eq!(StatusCode::OK, renamed.status);
    assert_eq!("Trip", renamed.json()["name"]);

    let updated = send(
        &router,
        json_request(
            Method::PUT,
            "/api/question-groups/g1/items/q1",
            r#"{"text":"Changed","questionType":"compare"}"#,
        ),
    )
    .await;
    assert_eq!(StatusCode::OK, updated.status);
    assert_eq!("Changed", updated.json()["questions"][0]["text"]);

    let removed = send(
        &router,
        empty(Method::DELETE, "/api/question-groups/g1/items/q2"),
    )
    .await;
    assert_eq!(StatusCode::OK, removed.status);
    assert_eq!(1, removed.json()["questions"].as_array().unwrap().len());

    let deleted = send(&router, empty(Method::DELETE, "/api/question-groups/g1")).await;
    assert_eq!(StatusCode::OK, deleted.status);
    assert!(deleted.body.is_empty());
    assert!(fixture.questions.groups().is_empty());
}

#[tokio::test]
async fn missing_group_is_not_found_with_message() {
    let fixture = Fixture::new();
    let reply = send(
        &fixture.router(),
        post_json("/api/question-groups/missing/items", r#"{"text":"q"}"#),
    )
    .await;

    assert_eq!(StatusCode::NOT_FOUND, reply.status);
    assert_eq!("No value present", reply.json()["message"]);
}

// ---- WrongNoteControllerTest

#[tokio::test]
async fn wrong_note_list_returns_mapped_response() {
    let fixture = Fixture::new();
    fixture.add_note(WrongNote::rehydrate(
        WrongNoteId::of("n1").unwrap(),
        "pattern",
        3,
        "summary",
        Some(Utc.with_ymd_and_hms(2026, 2, 1, 0, 0, 0).unwrap()),
    ));
    let reply = send(&fixture.router(), get("/api/wrongnotes")).await;

    assert_eq!(StatusCode::OK, reply.status);
    assert_eq!(
        json!([{
            "id": "n1",
            "pattern": "pattern",
            "count": 3,
            "shortSummary": "summary",
            "lastSeenAt": "2026-02-01T00:00:00Z"
        }]),
        reply.json()
    );
}

// ---- DataTransferControllerTest

#[tokio::test]
async fn export_data_returns_zip_attachment() {
    let fixture = Fixture::new();
    let reply = send(&fixture.router(), get("/api/data-transfer/export")).await;

    assert_eq!(StatusCode::OK, reply.status);
    let disposition = reply.headers[header::CONTENT_DISPOSITION].to_str().unwrap();
    assert!(disposition.contains("omypic-backup-"), "{disposition}");
    assert!(disposition.contains(".zip"), "{disposition}");
    assert_eq!("application/zip", reply.headers[header::CONTENT_TYPE]);
    assert_eq!(b"PK", &reply.body[..2]);
}

#[tokio::test]
async fn import_data_returns_import_summary() {
    let source = Fixture::new();
    source.add_group(group(
        "g1",
        "Travel",
        &["travel"],
        vec![("q1", "Q1", "habit")],
    ));
    source.add_note(WrongNote::rehydrate(
        WrongNoteId::of("n1").unwrap(),
        "pattern",
        3,
        "summary",
        None,
    ));
    let zip = send(&source.router(), get("/api/data-transfer/export"))
        .await
        .body;

    let target = Fixture::new();
    let reply = send(
        &target.router(),
        multipart(
            Method::POST,
            "/api/data-transfer/import",
            &[Part::File {
                name: "file",
                filename: "backup.zip",
                bytes: &zip,
            }],
        ),
    )
    .await;

    assert_eq!(StatusCode::OK, reply.status, "{}", reply.text());
    let body = reply.json();
    assert_eq!(1, body["questionGroupCount"]);
    assert_eq!(0, body["rulebookCount"]);
    assert_eq!(1, body["wrongNoteCount"]);
    assert_eq!(0, body["wrongNoteQueueSize"]);
    assert_eq!(json!(true), body["restartRequired"]);
    assert!(body["importedAt"].as_str().unwrap().ends_with('Z'));
    assert_eq!("g1", target.questions.groups()[0].id().value());
}

#[tokio::test]
async fn import_data_rejects_invalid_zip_with_bad_request() {
    let fixture = Fixture::new();
    let reply = send(
        &fixture.router(),
        multipart(
            Method::POST,
            "/api/data-transfer/import",
            &[Part::File {
                name: "file",
                filename: "backup.zip",
                bytes: &[9, 8, 7],
            }],
        ),
    )
    .await;

    assert_eq!(StatusCode::BAD_REQUEST, reply.status);
    assert!(reply.json()["message"].is_string());
}

#[tokio::test]
async fn import_data_without_file_part_is_bad_request() {
    let fixture = Fixture::new();
    let reply = send(
        &fixture.router(),
        multipart(
            Method::POST,
            "/api/data-transfer/import",
            &[Part::Text {
                name: "other",
                value: "x",
            }],
        ),
    )
    .await;

    assert_eq!(StatusCode::BAD_REQUEST, reply.status);
    assert_eq!(
        "Required part 'file' is not present.",
        reply.json()["message"]
    );
}

// ---- ModeControllerTest

#[tokio::test]
async fn update_uses_update_mode_with_selected_group_tags() {
    let fixture = Fixture::new();
    fixture.add_group(group(
        "g1",
        "Travel",
        &["travel"],
        vec![("q1", "Q1", "habit")],
    ));
    let reply = send(
        &fixture.router(),
        json_request(
            Method::PUT,
            "/api/modes",
            r#"{"sessionId":"s2","mode":"CONTINUOUS","continuousBatchSize":5,"selectedGroupTags":["travel"]}"#,
        ),
    )
    .await;

    assert_eq!(StatusCode::NO_CONTENT, reply.status);
    assert!(reply.body.is_empty());
    let state = fixture.sessions.get("s2").unwrap();
    let state = state.lock().unwrap();
    assert_eq!(ModeType::Continuous, state.mode());
    assert_eq!(5, state.continuous_batch_size());
    assert_eq!(&["travel".to_string()][..], state.selected_group_tags());
}

#[tokio::test]
async fn update_returns_bad_request_when_mode_is_unsupported() {
    let fixture = Fixture::new();
    let reply = send(
        &fixture.router(),
        json_request(
            Method::PUT,
            "/api/modes",
            r#"{"sessionId":"s3","mode":"MOCK_EXAM","selectedGroupTags":["travel"]}"#,
        ),
    )
    .await;

    assert_eq!(StatusCode::BAD_REQUEST, reply.status);
    assert!(reply.json()["message"].is_string());
}

#[tokio::test]
async fn update_returns_invalid_group_tags_when_selected_tags_are_empty() {
    let fixture = Fixture::new();
    let reply = send(
        &fixture.router(),
        json_request(
            Method::PUT,
            "/api/modes",
            r#"{"sessionId":"s4","mode":"IMMEDIATE","selectedGroupTags":[]}"#,
        ),
    )
    .await;

    assert_eq!(StatusCode::BAD_REQUEST, reply.status);
    assert_eq!(
        json!({
            "code": "INVALID_GROUP_TAGS",
            "message": "selectedGroupTags must not be empty",
            "invalidTags": []
        }),
        reply.json()
    );
}

#[tokio::test]
async fn update_returns_invalid_group_tags_when_some_tags_are_unknown() {
    let fixture = Fixture::new();
    fixture.add_group(group(
        "g1",
        "Travel",
        &["travel"],
        vec![("q1", "Q1", "habit")],
    ));
    let reply = send(
        &fixture.router(),
        json_request(
            Method::PUT,
            "/api/modes",
            r#"{"sessionId":"s5","mode":"IMMEDIATE","selectedGroupTags":["travel","unknown"]}"#,
        ),
    )
    .await;

    assert_eq!(StatusCode::BAD_REQUEST, reply.status);
    let body = reply.json();
    assert_eq!("INVALID_GROUP_TAGS", body["code"]);
    assert_eq!("unknown", body["invalidTags"][0]);
}

// ---- MetaControllerTest

fn meta_router(feedback: &str, stt: &str, tts: &str, voice: &str, language: &str) -> axum::Router {
    let mut fixture = Fixture::new();
    fixture.state.voice_policy =
        std::sync::Arc::new(omypic_application::test_support::FakeVoicePolicy {
            stt_model: stt.to_string(),
            feedback_model: feedback.to_string(),
            feedback_language: language.to_string(),
            tts_model: tts.to_string(),
            tts_voice: voice.to_string(),
            recovery_retention_ms: 600_000,
            stopped_context_max: 1000,
            idle_timeout_ms: 600_000,
        });
    fixture.router()
}

#[tokio::test]
async fn models_returns_supported_models_and_configured_defaults() {
    let router = meta_router(
        "gpt-5-nano",
        "gpt-4o-mini-transcribe",
        "gpt-4o-mini-tts",
        "alloy",
        "ko",
    );
    let reply = send(&router, get("/api/meta/models")).await;

    assert_eq!(StatusCode::OK, reply.status);
    let body = reply.json();
    assert_eq!("gpt-5-mini", body["feedbackModels"][0]);
    assert_eq!("gpt-5-nano", body["defaultFeedbackModel"]);
    assert_eq!("openai", body["feedbackProviders"][0]["provider"]);
    assert_eq!("gpt-5-nano", body["feedbackProviders"][0]["defaultModel"]);
    assert_eq!("anthropic", body["feedbackProviders"][1]["provider"]);
    assert_eq!("claude-opus-5-5", body["feedbackProviders"][1]["models"][0]);
    assert_eq!(
        "claude-opus-5-5",
        body["feedbackProviders"][1]["defaultModel"]
    );
    assert_eq!("gpt-4o-transcribe", body["voiceSttModels"][0]);
    assert_eq!("gpt-4o-mini-transcribe", body["defaultVoiceSttModel"]);
    assert_eq!("local-whisper", body["voiceSttModels"][3]);
    assert_eq!("local-kokoro", body["ttsModels"][3]);
    assert_eq!(4, body["ttsModels"].as_array().unwrap().len());
    assert!(!body["ttsModels"]
        .as_array()
        .unwrap()
        .iter()
        .any(|model| model == "device-voice"));
    assert_eq!("gpt-4o-mini-tts", body["ttsModels"][0]);
    assert_eq!("gpt-4o-mini-tts", body["defaultTtsModel"]);
    assert_eq!("alloy", body["voices"][0]);
    assert_eq!("alloy", body["defaultVoice"]);
    assert_eq!("ko", body["feedbackLanguages"][0]);
    assert_eq!("ko", body["defaultFeedbackLanguage"]);
}

#[tokio::test]
async fn models_falls_back_to_first_supported_when_configured_value_is_unknown() {
    let router = meta_router(
        "unknown-model",
        "unknown-stt",
        "unknown-tts",
        "unknown-voice",
        "unknown-lang",
    );
    let reply = send(&router, get("/api/meta/models")).await;

    assert_eq!(StatusCode::OK, reply.status);
    let body = reply.json();
    assert_eq!("gpt-5-mini", body["defaultFeedbackModel"]);
    assert_eq!("gpt-5-mini", body["feedbackProviders"][0]["defaultModel"]);
    assert_eq!("gpt-4o-transcribe", body["defaultVoiceSttModel"]);
    assert_eq!("gpt-4o-mini-tts", body["defaultTtsModel"]);
    assert_eq!("alloy", body["defaultVoice"]);
    assert_eq!("ko", body["defaultFeedbackLanguage"]);
}

// ---- RulebookControllerTest

fn rulebook(id: &str, scope: RulebookScope, enabled: bool) -> Rulebook {
    let at = Utc.with_ymd_and_hms(2026, 2, 1, 0, 0, 0).unwrap();
    Rulebook::rehydrate(
        RulebookId::of(id).unwrap(),
        "rules.md",
        "/tmp/rules.md",
        Some(scope),
        None,
        enabled,
        Some(at),
        Some(at),
    )
    .unwrap()
}

#[tokio::test]
async fn rulebook_list_returns_mapped_response() {
    let fixture = Fixture::new();
    fixture.add_rulebook(rulebook("r1", RulebookScope::Main, true));
    let reply = send(&fixture.router(), get("/api/rulebooks")).await;

    assert_eq!(StatusCode::OK, reply.status);
    assert_eq!(
        json!([{
            "id": "r1",
            "filename": "rules.md",
            "path": "/tmp/rules.md",
            "scope": "MAIN",
            "questionGroup": null,
            "enabled": true,
            "createdAt": "2026-02-01T00:00:00Z",
            "updatedAt": "2026-02-01T00:00:00Z"
        }]),
        reply.json()
    );
}

#[tokio::test]
async fn upload_passes_multipart_to_use_case() {
    let fixture = Fixture::new();
    let reply = send(
        &fixture.router(),
        multipart(
            Method::POST,
            "/api/rulebooks",
            &[
                Part::File {
                    name: "file",
                    filename: "rulebook.md",
                    bytes: b"hello",
                },
                Part::Text {
                    name: "scope",
                    value: "QUESTION",
                },
                Part::Text {
                    name: "questionGroup",
                    value: "A",
                },
            ],
        ),
    )
    .await;

    assert_eq!(StatusCode::OK, reply.status, "{}", reply.text());
    let body = reply.json();
    assert_eq!("rulebook.md", body["filename"]);
    assert_eq!("QUESTION", body["scope"]);
    assert_eq!("A", body["questionGroup"]);
    assert_eq!(json!(true), body["enabled"]);
    assert!(body["path"].as_str().unwrap().ends_with("rulebook.md"));
    let stored = fixture.files.files();
    assert!(stored.iter().any(|(_, bytes)| bytes == b"hello"));
}

#[tokio::test]
async fn upload_rejects_unknown_scope() {
    let fixture = Fixture::new();
    let reply = send(
        &fixture.router(),
        multipart(
            Method::POST,
            "/api/rulebooks",
            &[
                Part::File {
                    name: "file",
                    filename: "rulebook.md",
                    bytes: b"hello",
                },
                Part::Text {
                    name: "scope",
                    value: "SIDEWAYS",
                },
            ],
        ),
    )
    .await;

    assert_eq!(StatusCode::BAD_REQUEST, reply.status);
    assert!(fixture.rulebooks.rulebooks().is_empty());
}

#[tokio::test]
async fn upload_of_non_utf8_markdown_is_bad_request_and_stores_nothing() {
    let fixture = Fixture::new();
    let reply = send(
        &fixture.router(),
        multipart(
            Method::POST,
            "/api/rulebooks",
            &[Part::File {
                name: "file",
                filename: "rules.md",
                bytes: &[0xb1, 0xd4, 0xc4, 0xa2],
            }],
        ),
    )
    .await;

    assert_eq!(StatusCode::BAD_REQUEST, reply.status);
    assert_eq!(
        serde_json::json!({"message": "Rulebook must be UTF-8 text"}),
        reply.json()
    );
    assert!(fixture.rulebooks.rulebooks().is_empty());
    assert!(fixture.files.files().is_empty());
}

#[tokio::test]
async fn toggle_updates_enabled_flag() {
    let fixture = Fixture::new();
    fixture.add_rulebook(rulebook("r3", RulebookScope::Main, true));
    let router = fixture.router();

    let reply = send(
        &router,
        empty(Method::PUT, "/api/rulebooks/r3/toggle?enabled=false"),
    )
    .await;
    assert_eq!(StatusCode::OK, reply.status);
    assert_eq!(json!(false), reply.json()["enabled"]);

    let missing = send(&router, empty(Method::PUT, "/api/rulebooks/r3/toggle")).await;
    assert_eq!(StatusCode::BAD_REQUEST, missing.status);
    let invalid = send(
        &router,
        empty(Method::PUT, "/api/rulebooks/r3/toggle?enabled=maybe"),
    )
    .await;
    assert_eq!(StatusCode::BAD_REQUEST, invalid.status);
}

#[tokio::test]
async fn delete_delegates_to_use_case() {
    let fixture = Fixture::new();
    fixture.add_rulebook(rulebook("r4", RulebookScope::Main, true));
    let reply = send(
        &fixture.router(),
        empty(Method::DELETE, "/api/rulebooks/r4"),
    )
    .await;

    assert_eq!(StatusCode::OK, reply.status);
    assert_eq!(vec!["r4".to_string()], fixture.rulebooks.deleted_ids());
}

// ---- VoiceSessionControllerTest

const AI_OFF_OPEN: &str = r#"{"sessionId":"s3","aiEnabled":false}"#;

async fn open_voice_session(
    router: &axum::Router,
    body: &str,
    keys: &[(&str, &str)],
) -> (StatusCode, serde_json::Value) {
    let mut request = Request::builder()
        .method(Method::POST)
        .uri("/api/voice/sessions")
        .header(header::CONTENT_TYPE, "application/json");
    for (name, value) in keys {
        request = request.header(*name, *value);
    }
    let reply = send(router, request.body(Body::from(body.to_string())).unwrap()).await;
    (reply.status, reply.json())
}

/// 이벤트 스트림을 연결해 세션을 초기화한다(초기화는 첫 싱크가 붙을 때 일어난다).
async fn connect_events(
    router: &axum::Router,
    id: &str,
    expected: usize,
) -> (Body, Vec<(String, serde_json::Value)>) {
    let mut body = router
        .clone()
        .oneshot(get(&format!("/api/voice/sessions/{id}/events")))
        .await
        .unwrap()
        .into_body();
    let events = read_sse_events(&mut body, expected, Duration::from_secs(2)).await;
    assert_eq!(expected, events.len(), "{events:?}");
    (body, events)
}

/// 질문 그룹 하나와 그 태그를 고른 학습 모드를 세션에 적용한다.
async fn prepare_session(fixture: &Fixture, router: &axum::Router, session_id: &str) {
    fixture.add_group(group(
        "g-1",
        "travel",
        &["travel"],
        vec![("q-1", "question", "OPEN")],
    ));
    let reply = send(
        router,
        json_request(
            Method::PUT,
            "/api/modes",
            &format!(r#"{{"sessionId":"{session_id}","mode":"IMMEDIATE","selectedGroupTags":["travel"]}}"#),
        ),
    )
    .await;
    assert_eq!(StatusCode::NO_CONTENT, reply.status);
}

#[tokio::test]
async fn open_returns_voice_session_id() {
    let fixture = Fixture::new();
    let router = fixture.router();
    let (status, body) = open_voice_session(
        &router,
        r#"{"sessionId":"s1","feedbackModel":"gpt-4o-mini","feedbackLanguage":"ko","sttModel":"gpt-4o-mini-transcribe","ttsVoice":"alloy"}"#,
        &[("X-API-Key", "api-key")],
    )
    .await;

    assert_eq!(StatusCode::OK, status);
    let voice_session_id = body["voiceSessionId"].as_str().unwrap();
    assert!(!voice_session_id.is_empty());
    let recovery = send(
        &router,
        get(&format!("/api/voice/sessions/{voice_session_id}/recovery")),
    )
    .await;
    assert_eq!("s1", recovery.json()["sessionId"]);
    assert_eq!(voice_session_id, recovery.json()["voiceSessionId"]);
}

#[tokio::test]
async fn open_passes_feedback_provider_key_and_ai_toggle() {
    let fixture = Fixture::new();
    let router = fixture.router();
    let body = r#"{"sessionId":"s2","feedbackModel":"claude-opus-5-5","feedbackProvider":"anthropic","aiEnabled":false}"#;

    let (status, opened) = open_voice_session(
        &router,
        body,
        &[
            ("X-API-Key", "openai-key"),
            ("X-Feedback-API-Key", "sk-ant-key"),
        ],
    )
    .await;
    assert_eq!(StatusCode::OK, status);

    let id = opened["voiceSessionId"].as_str().unwrap();
    let mut stream = router
        .clone()
        .oneshot(get(&format!(
            "/api/voice/sessions/{id}/events?sinceEventId=0"
        )))
        .await
        .unwrap()
        .into_body();
    let events = read_sse_events(&mut stream, 1, Duration::from_secs(2)).await;
    assert_eq!("session.ready", events[0].0);
    assert_eq!(json!(false), events[0].1["aiEnabled"]);
    assert_eq!("anthropic", events[0].1["feedbackProvider"]);
    assert_eq!("s2", events[0].1["sessionId"]);

    // Claude 모델을 쓰는 AI 세션은 피드백 키 헤더가 있어야 열린다.
    let ai_on =
        r#"{"sessionId":"s2b","feedbackModel":"claude-opus-5-5","feedbackProvider":"anthropic"}"#;
    let (without_key, error) =
        open_voice_session(&router, ai_on, &[("X-API-Key", "openai-key")]).await;
    assert_eq!(StatusCode::BAD_REQUEST, without_key);
    assert_eq!(
        "Anthropic API key is required for Claude feedback models",
        error["message"]
    );
    let (with_key, _) = open_voice_session(
        &router,
        ai_on,
        &[
            ("X-API-Key", "openai-key"),
            ("X-Feedback-API-Key", "sk-ant-key"),
        ],
    )
    .await;
    assert_eq!(StatusCode::OK, with_key);
}

#[tokio::test]
async fn open_without_api_key_header_reaches_use_case_for_ai_off_sessions() {
    let fixture = Fixture::new();
    let router = fixture.router();
    let (status, body) = open_voice_session(&router, AI_OFF_OPEN, &[]).await;

    assert_eq!(StatusCode::OK, status);
    assert!(body["voiceSessionId"].is_string());

    let (status, error) = open_voice_session(&router, r#"{"sessionId":"s3"}"#, &[]).await;
    assert_eq!(StatusCode::BAD_REQUEST, status);
    assert_eq!("sessionId and apiKey are required", error["message"]);
}

#[tokio::test]
async fn open_requires_session_id() {
    let fixture = Fixture::new();
    let (status, error) = open_voice_session(&fixture.router(), r#"{"sessionId":" "}"#, &[]).await;

    assert_eq!(StatusCode::BAD_REQUEST, status);
    assert_eq!("sessionId must not be blank", error["message"]);
}

#[tokio::test]
async fn complete_answer_returns_no_content() {
    let fixture = Fixture::new();
    let router = fixture.router();
    let (_, opened) = open_voice_session(&router, AI_OFF_OPEN, &[]).await;
    let id = opened["voiceSessionId"].as_str().unwrap();
    fixture.add_group(group(
        "g1",
        "Travel",
        &["travel"],
        vec![("q1", "Q1", "habit")],
    ));

    let reply = send(
        &router,
        post_json(
            &format!("/api/voice/sessions/{id}/answers"),
            r#"{"sequence":4,"text":"my answer"}"#,
        ),
    )
    .await;
    assert_eq!(StatusCode::NO_CONTENT, reply.status);

    let unknown = send(
        &router,
        post_json("/api/voice/sessions/nope/answers", r#"{"sequence":4}"#),
    )
    .await;
    assert_eq!(StatusCode::BAD_REQUEST, unknown.status);
    assert_eq!("Unknown voice session: nope", unknown.json()["message"]);
}

#[tokio::test]
async fn append_audio_returns_no_content() {
    let fixture = Fixture::new();
    let router = fixture.router();
    prepare_session(&fixture, &router, "s1").await;
    let (_, opened) = open_voice_session(
        &router,
        r#"{"sessionId":"s1"}"#,
        &[("X-API-Key", "api-key")],
    )
    .await;
    let id = opened["voiceSessionId"].as_str().unwrap();
    let (mut stream, _) = connect_events(&router, id, 2).await;

    let reply = send(
        &router,
        post_json(
            &format!("/api/voice/sessions/{id}/audio-chunks"),
            r#"{"pcm16Base64":"AQI=","sampleRate":16000,"sequence":9}"#,
        ),
    )
    .await;

    assert_eq!(StatusCode::NO_CONTENT, reply.status);
    let recovery = send(&router, get(&format!("/api/voice/sessions/{id}/recovery")))
        .await
        .json();
    assert_eq!(9, recovery["lastAcceptedChunkSequence"]);
    // 청크는 바로 전사되어 턴이 처리된다.
    let events = read_sse_events(&mut stream, 2, Duration::from_secs(2)).await;
    let names: Vec<&str> = events.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(vec!["tts.audio", "stt.final"], names);
    assert_eq!("answer", events[1].1["text"]);
}

#[tokio::test]
async fn append_audio_requires_pcm16_base64() {
    let fixture = Fixture::new();
    let reply = send(
        &fixture.router(),
        post_json(
            "/api/voice/sessions/v1/audio-chunks",
            r#"{"pcm16Base64":""}"#,
        ),
    )
    .await;

    assert_eq!(StatusCode::BAD_REQUEST, reply.status);
    assert_eq!("pcm16Base64 must not be blank", reply.json()["message"]);
}

#[tokio::test]
async fn stop_without_body_defaults_to_forced_user_stop() {
    let fixture = Fixture::new();
    let router = fixture.router();
    let (_, opened) = open_voice_session(&router, AI_OFF_OPEN, &[]).await;
    let id = opened["voiceSessionId"].as_str().unwrap();

    let reply = send(
        &router,
        empty(Method::POST, &format!("/api/voice/sessions/{id}/stop")),
    )
    .await;

    assert_eq!(StatusCode::NO_CONTENT, reply.status);
    let recovery = send(&router, get(&format!("/api/voice/sessions/{id}/recovery")))
        .await
        .json();
    assert_eq!(json!(true), recovery["stopped"]);
    assert_eq!("user_stop", recovery["stopReason"]);
    let mut stream = router
        .clone()
        .oneshot(get(&format!(
            "/api/voice/sessions/{id}/events?sinceEventId=0"
        )))
        .await
        .unwrap()
        .into_body();
    let events = read_sse_events(&mut stream, 8, Duration::from_millis(300)).await;
    let stopped = events
        .iter()
        .find(|(name, _)| name == "session.stopped")
        .unwrap();
    assert_eq!(json!(true), stopped.1["forced"]);
    assert_eq!("user_stop", stopped.1["reason"]);
}

#[tokio::test]
async fn stop_with_body_passes_forced_and_reason() {
    let fixture = Fixture::new();
    let router = fixture.router();
    let (_, opened) = open_voice_session(&router, AI_OFF_OPEN, &[]).await;
    let id = opened["voiceSessionId"].as_str().unwrap();

    let reply = send(
        &router,
        post_json(
            &format!("/api/voice/sessions/{id}/stop"),
            r#"{"forced":false,"reason":"done"}"#,
        ),
    )
    .await;

    assert_eq!(StatusCode::NO_CONTENT, reply.status);
    let recovery = send(&router, get(&format!("/api/voice/sessions/{id}/recovery")))
        .await
        .json();
    assert_eq!("done", recovery["stopReason"]);
}

#[tokio::test]
async fn open_returns_bad_request_when_feedback_model_unsupported() {
    let fixture = Fixture::new();
    let (status, body) = open_voice_session(
        &fixture.router(),
        r#"{"sessionId":"s1","feedbackModel":"gpt-5.2"}"#,
        &[("X-API-Key", "api-key")],
    )
    .await;

    assert_eq!(StatusCode::BAD_REQUEST, status);
    assert_eq!("Unsupported feedback model: gpt-5.2", body["message"]);
}

#[tokio::test]
async fn recover_returns_snapshot() {
    let fixture = Fixture::new();
    let router = fixture.router();
    prepare_session(&fixture, &router, "s1").await;
    let (_, opened) =
        open_voice_session(&router, r#"{"sessionId":"s1","aiEnabled":false}"#, &[]).await;
    let id = opened["voiceSessionId"].as_str().unwrap();
    let _stream = connect_events(&router, id, 2).await;

    let reply = send(
        &router,
        get(&format!(
            "/api/voice/sessions/{id}/recovery?lastSeenEventId=0"
        )),
    )
    .await;

    assert_eq!(StatusCode::OK, reply.status);
    let body = reply.json();
    assert_eq!("s1", body["sessionId"]);
    assert_eq!(id, body["voiceSessionId"]);
    assert_eq!(json!(true), body["active"]);
    assert_eq!(json!(false), body["stopped"]);
    assert_eq!(1, body["currentTurnId"]);
    assert_eq!("q-1", body["currentQuestion"]["id"]);
    assert_eq!("question", body["currentQuestion"]["text"]);
    assert_eq!("travel", body["currentQuestion"]["group"]);
    assert_eq!("g-1", body["currentQuestion"]["groupId"]);
    assert_eq!("OPEN", body["currentQuestion"]["questionType"]);
    assert!(body["latestEventId"].as_i64().unwrap() >= 2);
    assert_eq!(0, body["replayFromEventId"]);
    assert_eq!(json!(false), body["gapDetected"]);
    assert_eq!(json!(null), body["lastAcceptedChunkSequence"]);
    for key in ["stopReason", "turnProcessing", "hasBufferedAudio"] {
        assert!(body.get(key).is_some(), "missing {key}");
    }
}

#[tokio::test]
async fn events_passes_since_event_id_to_use_case() {
    let fixture = Fixture::new();
    let router = fixture.router();
    prepare_session(&fixture, &router, "s3").await;
    let (_, opened) = open_voice_session(&router, AI_OFF_OPEN, &[]).await;
    let id = opened["voiceSessionId"].as_str().unwrap();

    let response = router
        .clone()
        .oneshot(get(&format!(
            "/api/voice/sessions/{id}/events?sinceEventId=0"
        )))
        .await
        .unwrap();
    assert_eq!(StatusCode::OK, response.status());
    assert!(response.headers()[header::CONTENT_TYPE]
        .to_str()
        .unwrap()
        .starts_with("text/event-stream"));
    let mut body = response.into_body();
    let replayed = read_sse_events(&mut body, 1, Duration::from_secs(2)).await;
    assert_eq!("session.ready", replayed[0].0);
    assert_eq!(1, replayed[0].1["eventId"]);

    let mut body = router
        .clone()
        .oneshot(get(&format!(
            "/api/voice/sessions/{id}/events?sinceEventId=1"
        )))
        .await
        .unwrap()
        .into_body();
    let later = read_sse_events(&mut body, 1, Duration::from_secs(2)).await;
    assert_eq!(
        "question.prompt", later[0].0,
        "event 1 must not be replayed"
    );
    assert_eq!(2, later[0].1["eventId"]);

    let invalid = send(
        &router,
        get(&format!("/api/voice/sessions/{id}/events?sinceEventId=abc")),
    )
    .await;
    assert_eq!(StatusCode::BAD_REQUEST, invalid.status);
    let unknown = send(&router, get("/api/voice/sessions/nope/events")).await;
    assert_eq!(StatusCode::BAD_REQUEST, unknown.status);
    assert_eq!("Unknown voice session: nope", unknown.json()["message"]);
}

// ---- WebConfigTest
// Java는 모든 origin(`*`)을 허용했다. 이제는 허용 목록의 origin만 받고 그 값을 그대로 돌려준다
// (다른 origin은 `tests/origin.rs`에서 403을 확인한다).

#[tokio::test]
async fn add_cors_mappings_allows_configured_origins_and_methods() {
    let fixture = Fixture::new();
    let request = Request::builder()
        .method(Method::OPTIONS)
        .uri("/api/question-groups")
        .header(header::ORIGIN, "http://localhost:5173")
        .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
        .body(Body::empty())
        .unwrap();
    let reply = send(&fixture.router(), request).await;

    assert_eq!(StatusCode::OK, reply.status);
    assert_eq!(
        "http://localhost:5173",
        reply.headers[header::ACCESS_CONTROL_ALLOW_ORIGIN]
    );
    assert!(reply.headers[header::ACCESS_CONTROL_ALLOW_METHODS]
        .to_str()
        .unwrap()
        .contains("GET"));
}
