#![allow(dead_code)]

use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, HeaderMap, Method, Request, StatusCode};
use axum::Router;
use chrono::{TimeZone, Utc};
use http_body_util::BodyExt;
use omypic_application::apikey::usecase::ApiKeyVerifyUseCase;
use omypic_application::datatransfer::usecase::DataTransferUseCase;
use omypic_application::feedback::port::LlmClient;
use omypic_application::feedback::usecase::FeedbackUseCase;
use omypic_application::question::port::QuestionGroupPort;
use omypic_application::question::usecase::QuestionUseCase;
use omypic_application::rulebook::port::RulebookPort;
use omypic_application::rulebook::usecase::RulebookUseCase;
use omypic_application::session::usecase::SessionUseCase;
use omypic_application::shared::local_model::{CachedModelFile, LocalModelError, LocalModelFiles};
use omypic_application::stt::usecase::SttUseCase;
use omypic_application::test_support::{
    FakeApiKeyProbePort, FakeFeedbackPolicy, FakeRagPolicy, FakeSttPolicy, FakeTtsGateway,
    FakeVoicePolicy, ImmediateExecutor, InMemoryDataReplacementPort, InMemoryQuestionGroupPort,
    InMemoryRulebookFileStore, InMemoryRulebookIndexPort, InMemoryRulebookPort,
    InMemorySessionStorePort, InMemoryWrongNotePort, InMemoryWrongNoteRecentQueuePort,
    ScriptedLlmClient, ScriptedSttGateway,
};
use omypic_application::voice::usecase::{VoiceSessionDependencies, VoiceSessionUseCase};
use omypic_application::wrongnote::port::WrongNotePort;
use omypic_application::wrongnote::usecase::WrongNoteUseCase;
use omypic_domain::error::AppError;
use omypic_domain::question::{
    QuestionGroupAggregate, QuestionGroupId, QuestionItem, QuestionItemId,
};
use omypic_domain::rulebook::Rulebook;
use omypic_domain::wrongnote::WrongNote;
use omypic_infrastructure::metrics::InMemoryMetrics;
use omypic_presentation::{router, AppState};
use serde_json::Value;
use tower::ServiceExt;

/// 서버 기본값(`AppConfig`)과 같은 목록.
pub const DEFAULT_ALLOWED_ORIGINS: [&str; 5] = [
    "http://localhost:5173",
    "http://127.0.0.1:5173",
    "tauri://localhost",
    "http://tauri.localhost",
    "https://tauri.localhost",
];

/// 모델 파일을 하나도 갖고 있지 않은 저장소. 모델 경로를 시험하는 쪽이 `state.local_models`를 바꾼다.
struct NoLocalModels;

impl LocalModelFiles for NoLocalModels {
    fn fetch(&self, repo: &str, _: &str, _: &str) -> Result<CachedModelFile, LocalModelError> {
        Err(LocalModelError::NotAllowed(format!(
            "Repository '{repo}' is not allowed"
        )))
    }
}

pub struct Fixture {
    pub state: AppState,
    pub questions: Arc<InMemoryQuestionGroupPort>,
    pub sessions: Arc<InMemorySessionStorePort>,
    pub rulebooks: Arc<InMemoryRulebookPort>,
    pub files: Arc<InMemoryRulebookFileStore>,
    pub wrong_notes: Arc<InMemoryWrongNotePort>,
    pub metrics: Arc<InMemoryMetrics>,
}

impl Fixture {
    pub fn new() -> Self {
        let questions = Arc::new(InMemoryQuestionGroupPort::new());
        Self::assemble(questions.clone(), questions)
    }

    pub fn with_question_port(port: Arc<dyn QuestionGroupPort>) -> Self {
        Self::assemble(port, Arc::new(InMemoryQuestionGroupPort::new()))
    }

    fn assemble(
        question_port: Arc<dyn QuestionGroupPort>,
        questions: Arc<InMemoryQuestionGroupPort>,
    ) -> Self {
        let sessions = Arc::new(InMemorySessionStorePort::new());
        let rulebooks = Arc::new(InMemoryRulebookPort::new());
        let files = Arc::new(InMemoryRulebookFileStore::new());
        let index = Arc::new(InMemoryRulebookIndexPort::new());
        let wrong_notes = Arc::new(InMemoryWrongNotePort::new());
        let queue = Arc::new(InMemoryWrongNoteRecentQueuePort::new());
        let metrics = Arc::new(InMemoryMetrics::new());
        let feedback_policy = Arc::new(FakeFeedbackPolicy::default());

        let rulebook = Arc::new(RulebookUseCase::new(
            rulebooks.clone(),
            index.clone(),
            files.clone(),
            Arc::new(FakeRagPolicy::default()),
            metrics.clone(),
        ));
        let wrong_note = Arc::new(WrongNoteUseCase::new(
            wrong_notes.clone(),
            queue.clone(),
            feedback_policy.clone(),
        ));
        let question = Arc::new(QuestionUseCase::new(
            question_port.clone(),
            sessions.clone(),
            metrics.clone(),
        ));
        let session = Arc::new(SessionUseCase::new(sessions.clone(), question_port.clone()));
        let clients: Vec<Arc<dyn LlmClient>> = vec![
            Arc::new(ScriptedLlmClient::new(
                "openai",
                false,
                vec![Err(AppError::Internal("unused".to_string()))],
            )),
            Arc::new(ScriptedLlmClient::new(
                "anthropic",
                true,
                vec![Err(AppError::Internal("unused".to_string()))],
            )),
        ];
        let feedback = Arc::new(
            FeedbackUseCase::new(
                clients,
                rulebook.clone(),
                feedback_policy,
                metrics.clone(),
                sessions.clone(),
                wrong_note.clone(),
            )
            .unwrap(),
        );
        let stt = Arc::new(SttUseCase::new(
            Arc::new(ScriptedSttGateway::new(vec![Ok("answer".to_string())])),
            Arc::new(FakeSttPolicy::default()),
            metrics.clone(),
        ));
        let voice_policy = Arc::new(FakeVoicePolicy::default());
        let voice_session = Arc::new(VoiceSessionUseCase::new(VoiceSessionDependencies {
            stt_use_case: stt,
            feedback_use_case: feedback,
            session_use_case: session.clone(),
            question_use_case: question.clone(),
            tts_gateway: Arc::new(FakeTtsGateway::new(&[1, 2, 3])),
            async_executor: Arc::new(ImmediateExecutor),
            voice_policy: voice_policy.clone(),
            metrics: metrics.clone(),
        }));
        let replacement = Arc::new(InMemoryDataReplacementPort::new(
            questions.clone(),
            rulebooks.clone(),
            wrong_notes.clone(),
            queue.clone(),
            index,
        ));
        let data_transfer = Arc::new(DataTransferUseCase::new(
            question_port,
            rulebooks.clone(),
            wrong_notes.clone(),
            queue,
            files.clone(),
            replacement,
        ));
        let state = AppState {
            api_key_verify: Arc::new(ApiKeyVerifyUseCase::new(Arc::new(
                FakeApiKeyProbePort::new(),
            ))),
            data_transfer,
            question,
            rulebook,
            session,
            voice_session,
            wrong_note,
            voice_policy,
            metrics: metrics.clone(),
            local_models: Arc::new(NoLocalModels),
            allowed_origins: Arc::new(
                DEFAULT_ALLOWED_ORIGINS
                    .iter()
                    .map(|origin| origin.to_string())
                    .collect(),
            ),
        };
        Self {
            state,
            questions,
            sessions,
            rulebooks,
            files,
            wrong_notes,
            metrics,
        }
    }

    pub fn add_group(&self, group: QuestionGroupAggregate) {
        self.questions.save(group).unwrap();
    }

    pub fn add_note(&self, note: WrongNote) {
        self.wrong_notes.save(note).unwrap();
    }

    pub fn add_rulebook(&self, rulebook: Rulebook) {
        self.rulebooks.save(rulebook).unwrap();
    }

    pub fn router(&self) -> Router {
        router(self.state.clone())
    }
}

pub struct Reply {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Vec<u8>,
}

impl Reply {
    pub fn json(&self) -> Value {
        serde_json::from_slice(&self.body)
            .unwrap_or_else(|e| panic!("body is not JSON ({e}): {:?}", self.text()))
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

pub async fn send(router: &Router, request: Request<Body>) -> Reply {
    let response = router.clone().oneshot(request).await.unwrap();
    let (parts, body) = response.into_parts();
    let body = body.collect().await.unwrap().to_bytes().to_vec();
    Reply {
        status: parts.status,
        headers: parts.headers,
        body,
    }
}

pub fn get(uri: &str) -> Request<Body> {
    Request::builder().uri(uri).body(Body::empty()).unwrap()
}

pub fn json_request(method: Method, uri: &str, body: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

pub fn post_json(uri: &str, body: &str) -> Request<Body> {
    json_request(Method::POST, uri, body)
}

pub fn empty(method: Method, uri: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .body(Body::empty())
        .unwrap()
}

const BOUNDARY: &str = "----omypic-test-boundary";

pub enum Part<'a> {
    File {
        name: &'a str,
        filename: &'a str,
        bytes: &'a [u8],
    },
    Text {
        name: &'a str,
        value: &'a str,
    },
}

pub fn multipart(method: Method, uri: &str, parts: &[Part]) -> Request<Body> {
    let mut body: Vec<u8> = Vec::new();
    for part in parts {
        body.extend_from_slice(format!("--{BOUNDARY}\r\n").as_bytes());
        match part {
            Part::File {
                name,
                filename,
                bytes,
            } => {
                body.extend_from_slice(
                    format!(
                        "Content-Disposition: form-data; name=\"{name}\"; filename=\"{filename}\"\r\nContent-Type: application/octet-stream\r\n\r\n"
                    )
                    .as_bytes(),
                );
                body.extend_from_slice(bytes);
            }
            Part::Text { name, value } => {
                body.extend_from_slice(
                    format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}")
                        .as_bytes(),
                );
            }
        }
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{BOUNDARY}--\r\n").as_bytes());
    Request::builder()
        .method(method)
        .uri(uri)
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={BOUNDARY}"),
        )
        .body(Body::from(body))
        .unwrap()
}

pub fn group(
    id: &str,
    name: &str,
    tags: &[&str],
    questions: Vec<(&str, &str, &str)>,
) -> QuestionGroupAggregate {
    let at = Utc.with_ymd_and_hms(2026, 2, 1, 0, 0, 0).unwrap();
    QuestionGroupAggregate::rehydrate(
        QuestionGroupId::of(id).unwrap(),
        name,
        tags.iter().copied(),
        questions
            .into_iter()
            .map(|(item_id, text, question_type)| {
                QuestionItem::rehydrate(
                    QuestionItemId::of(item_id).unwrap(),
                    text,
                    Some(question_type),
                )
            })
            .collect(),
        Some(at),
        Some(at),
    )
    .unwrap()
}

/// SSE 본문에서 이벤트 프레임을 `count`개 읽는다. 시간 안에 못 읽으면 읽은 만큼만 돌려준다.
pub async fn read_sse_events(
    body: &mut Body,
    count: usize,
    timeout: std::time::Duration,
) -> Vec<(String, Value)> {
    let mut buffer = String::new();
    let mut events = Vec::new();
    let deadline = tokio::time::Instant::now() + timeout;
    while events.len() < count {
        while let Some(end) = buffer.find("\n\n") {
            let frame: String = buffer.drain(..end + 2).collect();
            events.push(parse_sse_frame(&frame));
        }
        if events.len() >= count {
            break;
        }
        match tokio::time::timeout_at(deadline, body.frame()).await {
            Ok(Some(Ok(frame))) => {
                if let Some(data) = frame.data_ref() {
                    buffer.push_str(&String::from_utf8_lossy(data));
                }
            }
            _ => break,
        }
    }
    events
}

fn parse_sse_frame(frame: &str) -> (String, Value) {
    let mut name = String::new();
    let mut data = String::new();
    for line in frame.lines() {
        if let Some(value) = line.strip_prefix("event:") {
            name = value.trim().to_string();
        } else if let Some(value) = line.strip_prefix("data:") {
            data.push_str(value.trim_start());
        }
    }
    (name, serde_json::from_str(&data).unwrap_or(Value::Null))
}
