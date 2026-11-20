//! Java `VoiceSessionUseCaseTest`. Mockito mock 대신 실제 유스케이스를 `test_support` 가짜 위에
//! 만든다.
//!
//! - `questionUseCase.nextQuestion(...)` 대본은 질문 하나씩 든 질문 그룹과, 그 순서로 후보를
//!   정해 둔 세션 상태로 바꿨다. 대본이 끝나면 실제 유스케이스는 `skipped`를 돌려준다.
//! - `feedbackUseCase`는 실제 `FeedbackUseCase`다. 피드백 요청은 LLM 가짜의 호출 기록으로,
//!   프리페치를 썼는지는 룰북 색인 가짜의 검색 질의로 확인한다. 프리페치는 질문만으로 찾고
//!   (`question-1`), 프리페치 없이 만든 피드백은 질문과 답으로 찾는다(`question-1\nanswer text`).
//! - 실제 프리페치는 실패하지 않으므로 Java 기본(mock이 `null`을 돌려 프리페치가 없는 경우)과
//!   달리 피드백은 보통 프리페치로 만들어진다. 프리페치가 없는 경우는
//!   `feedback_searches_rulebooks_itself_when_prefetch_has_not_finished`가 따로 본다.

use std::sync::atomic::{AtomicBool, AtomicI64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use chrono::{TimeZone, Utc};
use omypic_domain::error::AppError;
use omypic_domain::feedback::Feedback;
use omypic_domain::question::{
    QuestionGroupAggregate, QuestionGroupId, QuestionItem, QuestionItemId,
};
use omypic_domain::rulebook::{Rulebook, RulebookId, RulebookScope};
use omypic_domain::session::{ModeType, SessionId, SessionState};
use serde_json::Value;

use super::{RecoverySnapshot, VoiceSessionDependencies, VoiceSessionUseCase};
use crate::feedback::port::{LlmClient, LlmGenerateResult};
use crate::feedback::usecase::FeedbackUseCase;
use crate::question::port::QuestionGroupPort;
use crate::question::usecase::QuestionUseCase;
use crate::rulebook::usecase::RulebookUseCase;
use crate::session::usecase::SessionUseCase;
use crate::shared::port::AsyncExecutor;
use crate::stt::model::VadSettings;
use crate::stt::port::SttGateway;
use crate::stt::usecase::SttUseCase;
use crate::test_support::{
    DeferredExecutor, FakeFeedbackPolicy, FakeRagPolicy, FakeSttPolicy, FakeTtsGateway,
    FakeVoicePolicy, ImmediateExecutor, InMemoryQuestionGroupPort, InMemoryRulebookFileStore,
    InMemoryRulebookIndexPort, InMemoryRulebookPort, InMemorySessionStorePort,
    InMemoryWrongNotePort, InMemoryWrongNoteRecentQueuePort, RecordingMetrics, RecordingSink,
    ScriptedLlmClient, ScriptedSttGateway,
};
use crate::voice::model::{
    VoiceAnswerCommand, VoiceAudioChunkCommand, VoiceSessionOpenCommand, VoiceSessionStopCommand,
};
use crate::wrongnote::usecase::WrongNoteUseCase;

type Events = Vec<(String, Value)>;
pub(super) type Question = (&'static str, &'static str, &'static str, &'static str);

pub(super) const Q1: Question = ("q-1", "question-1", "g-1", "travel");
pub(super) const Q2: Question = ("q-2", "question-2", "g-2", "hobby");
const Q3: Question = ("q-3", "question-3", "g-3", "music");

/// Java `setUp`의 `voicePolicy` 값.
pub(super) fn java_voice_policy() -> FakeVoicePolicy {
    FakeVoicePolicy {
        stt_model: "gpt-4o-mini-transcribe".to_string(),
        feedback_model: "gpt-4o-mini".to_string(),
        feedback_language: "ko".to_string(),
        tts_model: "gpt-4o-mini-tts".to_string(),
        tts_voice: "alloy".to_string(),
        recovery_retention_ms: 600_000,
        stopped_context_max: 1000,
        idle_timeout_ms: 600_000,
    }
}

pub(super) struct Fixture {
    sessions: Arc<InMemorySessionStorePort>,
    questions: Arc<InMemoryQuestionGroupPort>,
    stt: Arc<ScriptedSttGateway>,
    tts: Arc<FakeTtsGateway>,
    open_ai: Arc<ScriptedLlmClient>,
    anthropic: Arc<ScriptedLlmClient>,
    index: Arc<InMemoryRulebookIndexPort>,
    feedback_failure: Arc<Mutex<Option<AppError>>>,
    /// 참이면 다음 피드백 요청 하나가 패닉한다.
    feedback_panic: Arc<AtomicBool>,
    pub(super) deps: VoiceSessionDependencies,
}

impl Fixture {
    pub(super) fn new() -> Self {
        Self::with(java_voice_policy(), Arc::new(ImmediateExecutor))
    }

    pub(super) fn with(policy: FakeVoicePolicy, executor: Arc<dyn AsyncExecutor>) -> Self {
        Self::with_tts_audio(policy, executor, &[1, 2, 3, 4])
    }

    /// 서버 TTS가 돌려줄 WAV 바이트를 정한다.
    pub(super) fn with_tts_audio(
        policy: FakeVoicePolicy,
        executor: Arc<dyn AsyncExecutor>,
        tts_audio: &[u8],
    ) -> Self {
        let feedback_failure: Arc<Mutex<Option<AppError>>> = Arc::new(Mutex::new(None));
        let feedback_panic = Arc::new(AtomicBool::new(false));
        let open_ai = Arc::new(feedback_client(
            "openai",
            false,
            &feedback_failure,
            &feedback_panic,
        ));
        let anthropic = Arc::new(feedback_client(
            "anthropic",
            true,
            &feedback_failure,
            &feedback_panic,
        ));
        let sessions = Arc::new(InMemorySessionStorePort::new());
        let questions = Arc::new(InMemoryQuestionGroupPort::new());
        let stt = Arc::new(ScriptedSttGateway::new(vec![Ok("answer text".to_string())]));
        let tts = Arc::new(FakeTtsGateway::new(tts_audio));
        let index = Arc::new(InMemoryRulebookIndexPort::new());
        let metrics = Arc::new(RecordingMetrics::new());
        let rulebook_use_case = Arc::new(RulebookUseCase::new(
            Arc::new(InMemoryRulebookPort::with_rulebooks(vec![main_rulebook()])),
            index.clone(),
            Arc::new(InMemoryRulebookFileStore::new()),
            Arc::new(FakeRagPolicy::default()),
            metrics.clone(),
        ));
        let feedback_policy = Arc::new(FakeFeedbackPolicy::default());
        let wrong_note_use_case = Arc::new(WrongNoteUseCase::new(
            Arc::new(InMemoryWrongNotePort::new()),
            Arc::new(InMemoryWrongNoteRecentQueuePort::new()),
            feedback_policy.clone(),
        ));
        let clients: Vec<Arc<dyn LlmClient>> = vec![open_ai.clone(), anthropic.clone()];
        let feedback_use_case = Arc::new(
            FeedbackUseCase::new(
                clients,
                rulebook_use_case,
                feedback_policy,
                metrics.clone(),
                sessions.clone(),
                wrong_note_use_case,
            )
            .unwrap(),
        );
        let deps = VoiceSessionDependencies {
            stt_use_case: Arc::new(SttUseCase::new(
                stt.clone(),
                Arc::new(FakeSttPolicy::default()),
                metrics.clone(),
            )),
            feedback_use_case,
            session_use_case: Arc::new(SessionUseCase::new(sessions.clone(), questions.clone())),
            question_use_case: Arc::new(QuestionUseCase::new(
                questions.clone(),
                sessions.clone(),
                metrics.clone(),
            )),
            tts_gateway: tts.clone(),
            async_executor: executor,
            voice_policy: Arc::new(policy),
            metrics,
        };
        Self {
            sessions,
            questions,
            stt,
            tts,
            open_ai,
            anthropic,
            index,
            feedback_failure,
            feedback_panic,
            deps,
        }
    }

    pub(super) fn use_case(&self) -> VoiceSessionUseCase {
        VoiceSessionUseCase::new(self.deps.clone())
    }

    /// Java `when(sessionUseCase.getOrCreate(id)).thenReturn(state)`와
    /// `when(questionUseCase.nextQuestion(id)).thenReturn(...)`: 질문마다 그룹 하나를 만들고
    /// 그 순서대로 묻는 세션을 둔다. 질문이 다 떨어지면 `skipped`다.
    pub(super) fn session(
        &self,
        id: &str,
        mode: Option<ModeType>,
        batch: Option<i32>,
        questions: &[Question],
    ) {
        let at = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        for (question_id, text, group_id, group) in questions {
            let item = QuestionItem::rehydrate(
                QuestionItemId::of(question_id).unwrap(),
                text,
                Some("OPEN"),
            );
            let aggregate = QuestionGroupAggregate::rehydrate(
                QuestionGroupId::of(group_id).unwrap(),
                group,
                ["voice"],
                vec![item],
                Some(at),
                Some(at),
            )
            .unwrap();
            self.questions.save(aggregate).unwrap();
        }
        let mut state = SessionState::new(SessionId::of(id).unwrap());
        state.apply_mode_update(mode, batch);
        state.configure_question_groups(["voice"], questions.iter().map(|q| q.2));
        self.sessions.insert(state);
    }

    fn fail_feedback_with(&self, error: AppError) {
        *self.feedback_failure.lock().unwrap() = Some(error);
    }

    /// 룰북 검색(`search_contexts_for_turn` 한 번)마다 질의 하나. 검색 한 번이 색인을 여러 번
    /// 부르므로 잇달아 같은 질의는 하나로 센다.
    fn search_queries(&self) -> Vec<String> {
        let mut queries: Vec<String> = self
            .index
            .search_calls()
            .into_iter()
            .map(|call| call.query)
            .collect();
        queries.dedup();
        queries
    }

    /// 이 질문과 답으로 피드백을 요청했는가 (추천 표현 보강 요청도 같은 질문과 답을 담는다).
    fn requested_feedback(client: &ScriptedLlmClient, question: &str, answer: &str) -> bool {
        client
            .generate_calls()
            .iter()
            .any(|call| call.user_prompt.contains(question) && call.user_prompt.contains(answer))
    }
}

fn feedback_client(
    provider: &str,
    stateless: bool,
    failure: &Arc<Mutex<Option<AppError>>>,
    panic_once: &Arc<AtomicBool>,
) -> ScriptedLlmClient {
    let failure = failure.clone();
    let panic_once = panic_once.clone();
    ScriptedLlmClient::with_responder(provider, stateless, move |_| {
        if panic_once.swap(false, Ordering::SeqCst) {
            panic!("feedback client exploded");
        }
        if let Some(error) = failure.lock().unwrap().clone() {
            return Err(error);
        }
        Ok(LlmGenerateResult::new(
            Some(sample_feedback()),
            None,
            None,
            None,
        ))
    })
}

fn main_rulebook() -> Rulebook {
    let at = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    Rulebook::rehydrate(
        RulebookId::of("r-main").unwrap(),
        "main.md",
        "/tmp/main.md",
        Some(RulebookScope::Main),
        None,
        true,
        Some(at),
        Some(at),
    )
    .unwrap()
}

fn sample_feedback() -> Feedback {
    Feedback::of_correction_points(
        "summary",
        &[
            "point-1".to_string(),
            "point-2".to_string(),
            "point-3".to_string(),
        ],
        "example",
        &[],
    )
}

pub(super) fn open_command(session_id: &str) -> VoiceSessionOpenCommand {
    VoiceSessionOpenCommand {
        session_id: Some(session_id.to_string()),
        api_key: Some("api-key".to_string()),
        ..VoiceSessionOpenCommand::default()
    }
}

pub(super) fn ai_off_command(session_id: &str) -> VoiceSessionOpenCommand {
    VoiceSessionOpenCommand {
        session_id: Some(session_id.to_string()),
        ai_enabled: Some(false),
        ..VoiceSessionOpenCommand::default()
    }
}

pub(super) fn answer(voice_session_id: &str, sequence: i64) -> VoiceAnswerCommand {
    VoiceAnswerCommand {
        voice_session_id: Some(voice_session_id.to_string()),
        sequence: Some(sequence),
        text: None,
    }
}

pub(super) fn audio_chunk(voice_session_id: &str, sequence: i64) -> VoiceAudioChunkCommand {
    VoiceAudioChunkCommand {
        voice_session_id: Some(voice_session_id.to_string()),
        // Base64.getEncoder().encodeToString(new byte[]{0, 1, 2, 3})
        pcm16_base64: Some("AAECAw==".to_string()),
        sample_rate: Some(16000),
        sequence: Some(sequence),
    }
}

pub(super) fn stop_command(voice_session_id: &str) -> VoiceSessionStopCommand {
    VoiceSessionStopCommand {
        voice_session_id: Some(voice_session_id.to_string()),
        ..VoiceSessionStopCommand::default()
    }
}

fn first_index(events: &Events, predicate: impl Fn(&(String, Value)) -> bool) -> Option<usize> {
    events.iter().position(predicate)
}

fn of_type(name: &str) -> impl Fn(&(String, Value)) -> bool + '_ {
    move |(event, _)| event == name
}

fn is_tts_role<'a>(role: &'a str) -> impl Fn(&(String, Value)) -> bool + 'a {
    move |(event, payload)| {
        if event != "tts.audio" {
            return false;
        }
        let value = payload.get("role").or_else(|| payload.get("phase"));
        value == Some(&Value::from(role))
    }
}

fn event_id(payload: &Value) -> i64 {
    payload["eventId"].as_i64().expect("eventId is a number")
}

pub(super) fn unknown_session(voice_session_id: &str) -> AppError {
    AppError::InvalidArgument(format!("Unknown voice session: {voice_session_id}"))
}

#[test]
fn immediate_mode_emits_feedback_speech_before_next_question_speech() {
    let f = Fixture::new();
    f.session("s1", Some(ModeType::Immediate), None, &[Q1, Q2]);
    let use_case = f.use_case();

    let voice_session_id = use_case.open(&open_command("s1")).unwrap();
    let sink = Arc::new(RecordingSink::new());
    use_case
        .register_sink(&voice_session_id, sink.clone())
        .unwrap();
    assert_eq!(1, f.open_ai.bootstrap_calls().len());

    sink.clear();
    use_case
        .append_audio(&audio_chunk(&voice_session_id, 1))
        .unwrap();
    let events = sink.events();

    let feedback_final = first_index(&events, of_type("feedback.final")).unwrap();
    let feedback_speech = first_index(&events, is_tts_role("feedback")).unwrap();
    let question_prompt = first_index(&events, of_type("question.prompt")).unwrap();
    let question_speech = first_index(&events, is_tts_role("question")).unwrap();
    assert!(feedback_final < feedback_speech);
    assert!(feedback_speech < question_prompt);
    assert!(question_prompt < question_speech);

    let feedback_sequence = events[feedback_speech].1["sequence"].as_i64().unwrap();
    let question_sequence = events[question_speech].1["sequence"].as_i64().unwrap();
    assert!(feedback_sequence < question_sequence);
}

#[test]
fn continuous_mode_keeps_feedback_as_text_only_and_speaks_question_only() {
    let f = Fixture::new();
    f.session("s1", Some(ModeType::Continuous), Some(1), &[Q1, Q2]);
    let use_case = f.use_case();

    let voice_session_id = use_case.open(&open_command("s1")).unwrap();
    let sink = Arc::new(RecordingSink::new());
    use_case
        .register_sink(&voice_session_id, sink.clone())
        .unwrap();

    sink.clear();
    use_case
        .append_audio(&audio_chunk(&voice_session_id, 1))
        .unwrap();
    let events = sink.events();

    let question_prompt = first_index(&events, of_type("question.prompt")).unwrap();
    let question_speech = first_index(&events, is_tts_role("question")).unwrap();
    let feedback_final = first_index(&events, of_type("feedback.final")).unwrap();
    let question_speech_count = events.iter().filter(|e| is_tts_role("question")(e)).count();
    assert!(question_prompt < question_speech);
    assert!(question_speech < feedback_final);
    assert_eq!(None, first_index(&events, is_tts_role("feedback")));
    assert_eq!(1, question_speech_count);
}

#[test]
fn immediate_mode_uses_prefetched_turn_prompt_when_available() {
    let f = Fixture::new();
    f.session("s1", Some(ModeType::Immediate), None, &[Q1, Q2]);
    let use_case = f.use_case();

    let voice_session_id = use_case.open(&open_command("s1")).unwrap();
    let sink = Arc::new(RecordingSink::new());
    use_case
        .register_sink(&voice_session_id, sink.clone())
        .unwrap();

    sink.clear();
    use_case
        .append_audio(&audio_chunk(&voice_session_id, 1))
        .unwrap();

    // generateFeedbackForTurnWithPrefetch: 피드백은 만들어졌고,
    assert!(Fixture::requested_feedback(
        &f.open_ai,
        "question-1",
        "answer text"
    ));
    // 룰북은 프리페치가 질문만으로 찾았다.
    let queries = f.search_queries();
    assert!(
        queries.iter().any(|query| query == "question-1"),
        "{queries:?}"
    );
    // generateFeedbackForTurn never: 질문과 답으로 다시 찾지 않았다.
    assert!(
        queries.iter().all(|query| !query.contains("answer text")),
        "{queries:?}"
    );
}

#[test]
fn immediate_mode_continues_to_next_question_when_feedback_fails() {
    let f = Fixture::new();
    f.session("s1", Some(ModeType::Immediate), None, &[Q1, Q2]);
    f.fail_feedback_with(AppError::InvalidState("timeout".to_string()));
    let use_case = f.use_case();

    let voice_session_id = use_case.open(&open_command("s1")).unwrap();
    let sink = Arc::new(RecordingSink::new());
    use_case
        .register_sink(&voice_session_id, sink.clone())
        .unwrap();

    sink.clear();
    use_case
        .append_audio(&audio_chunk(&voice_session_id, 1))
        .unwrap();
    let events = sink.events();

    let error = first_index(&events, of_type("session.error")).unwrap();
    let question_prompt = first_index(&events, of_type("question.prompt")).unwrap();
    assert_eq!(None, first_index(&events, of_type("feedback.final")));
    assert!(first_index(&events, is_tts_role("question")).is_some());
    assert!(error < question_prompt);
    assert_eq!(
        "피드백 생성에 실패했습니다. 다음 질문으로 진행합니다. (LLM feedback failed: timeout)",
        events[error].1["message"]
    );
}

#[test]
fn append_audio_ignores_duplicate_or_older_sequence() {
    let f = Fixture::new();
    f.session("s1", Some(ModeType::Immediate), None, &[Q1, Q2]);
    let use_case = f.use_case();

    let voice_session_id = use_case.open(&open_command("s1")).unwrap();
    use_case
        .register_sink(&voice_session_id, Arc::new(RecordingSink::new()))
        .unwrap();

    use_case
        .append_audio(&audio_chunk(&voice_session_id, 5))
        .unwrap();
    use_case
        .append_audio(&audio_chunk(&voice_session_id, 5))
        .unwrap();
    use_case
        .append_audio(&audio_chunk(&voice_session_id, 4))
        .unwrap();

    assert_eq!(1, f.stt.calls().len());
}

#[test]
fn initialize_session_stops_when_bootstrap_fails() {
    let f = Fixture::new();
    f.session("s1", Some(ModeType::Immediate), None, &[]);
    f.open_ai
        .set_bootstrap_result(Err(AppError::InvalidState("bootstrap failed".to_string())));
    let use_case = f.use_case();

    let voice_session_id = use_case.open(&open_command("s1")).unwrap();
    let sink = Arc::new(RecordingSink::new());
    use_case
        .register_sink(&voice_session_id, sink.clone())
        .unwrap();
    let events = sink.events();

    assert_eq!(None, first_index(&events, of_type("session.ready")));
    let error = first_index(&events, of_type("session.error")).unwrap();
    let stopped = first_index(&events, of_type("session.stopped")).unwrap();
    assert!(stopped > error);
    assert_eq!("bootstrap failed", events[error].1["message"]);
    assert_eq!("initialization_failed", events[stopped].1["reason"]);
}

#[test]
fn emitted_events_include_monotonic_event_id() {
    let f = Fixture::new();
    f.session("s1", Some(ModeType::Immediate), None, &[Q1, Q2]);
    let use_case = f.use_case();

    let voice_session_id = use_case.open(&open_command("s1")).unwrap();
    let sink = Arc::new(RecordingSink::new());
    use_case
        .register_sink(&voice_session_id, sink.clone())
        .unwrap();

    sink.clear();
    use_case
        .append_audio(&audio_chunk(&voice_session_id, 1))
        .unwrap();

    let events = sink.events();
    assert!(!events.is_empty());
    let mut previous = 0;
    for (_, payload) in &events {
        let id = event_id(payload);
        assert!(id > previous);
        previous = id;
    }
}

#[test]
fn register_sink_replays_events_after_since_event_id() {
    let f = Fixture::new();
    f.session("s1", Some(ModeType::Immediate), None, &[Q1, Q2]);
    let use_case = f.use_case();

    let voice_session_id = use_case.open(&open_command("s1")).unwrap();
    let sink = Arc::new(RecordingSink::new());
    use_case
        .register_sink(&voice_session_id, sink.clone())
        .unwrap();
    use_case
        .append_audio(&audio_chunk(&voice_session_id, 1))
        .unwrap();

    let events = sink.events();
    let since_event_id = event_id(&events[0].1);
    let expected: Vec<i64> = events
        .iter()
        .map(|(_, payload)| event_id(payload))
        .filter(|id| *id > since_event_id)
        .collect();

    let replayed = Arc::new(RecordingSink::new());
    use_case
        .register_sink_since(&voice_session_id, replayed.clone(), Some(since_event_id))
        .unwrap();
    let replayed_ids: Vec<i64> = replayed
        .events()
        .iter()
        .map(|(_, payload)| event_id(payload))
        .collect();

    assert!(!expected.is_empty());
    assert_eq!(expected, replayed_ids);
    assert_eq!(events[1..].to_vec(), replayed.events());
}

#[test]
fn recover_marks_gap_detected_when_replay_buffer_trimmed() {
    let f = Fixture::new();
    let small_replay_use_case =
        VoiceSessionUseCase::with_event_replay_buffer_limit(f.deps.clone(), 4);
    f.session(
        "s1",
        Some(ModeType::Immediate),
        None,
        &[
            Q1,
            Q2,
            ("q-3", "question-3", "g-3", "hobby"),
            ("q-4", "question-4", "g-4", "hobby"),
            ("q-5", "question-5", "g-5", "hobby"),
            ("q-6", "question-6", "g-6", "hobby"),
            ("q-7", "question-7", "g-7", "hobby"),
        ],
    );

    let voice_session_id = small_replay_use_case.open(&open_command("s1")).unwrap();
    small_replay_use_case
        .register_sink(&voice_session_id, Arc::new(RecordingSink::new()))
        .unwrap();
    for sequence in 1..=6 {
        small_replay_use_case
            .append_audio(&audio_chunk(&voice_session_id, sequence))
            .unwrap();
    }

    let snapshot = small_replay_use_case
        .recover(&voice_session_id, Some(0))
        .unwrap();
    assert!(snapshot.gap_detected);
    assert_eq!(snapshot.latest_event_id - 4, snapshot.replay_from_event_id);
    assert!(snapshot.current_question.is_some());
}

#[test]
fn purge_stopped_contexts_removes_expired_session_after_ttl() {
    let f = Fixture::new();
    let now = Arc::new(AtomicI64::new(1_000));
    let clock = now.clone();
    let ttl_use_case = VoiceSessionUseCase::with_clock(
        f.deps.clone(),
        256,
        Arc::new(move || clock.load(Ordering::SeqCst)),
    );

    let target = ttl_use_case.open(&open_command("s-ttl")).unwrap();
    ttl_use_case.stop(&stop_command(&target)).unwrap();

    let before_expiry = ttl_use_case.recover(&target, Some(0)).unwrap();
    assert!(before_expiry.stopped);

    now.fetch_add(600_001, Ordering::SeqCst);
    ttl_use_case.open(&open_command("s-trigger")).unwrap();

    let error = ttl_use_case.recover(&target, Some(0)).unwrap_err();
    assert_eq!(unknown_session(&target), error);
}

#[test]
fn purge_stopped_contexts_enforces_stopped_context_limit_without_removing_active() {
    let policy = FakeVoicePolicy {
        recovery_retention_ms: 10_000_000,
        stopped_context_max: 2,
        ..java_voice_policy()
    };
    let f = Fixture::with(policy, Arc::new(ImmediateExecutor));
    let now = Arc::new(AtomicI64::new(10_000));
    let clock = now.clone();
    let limited_use_case = VoiceSessionUseCase::with_clock(
        f.deps.clone(),
        256,
        Arc::new(move || clock.load(Ordering::SeqCst)),
    );

    let active = limited_use_case.open(&open_command("s-active")).unwrap();
    let stopped_a = limited_use_case.open(&open_command("s-a")).unwrap();
    limited_use_case.stop(&stop_command(&stopped_a)).unwrap();
    now.fetch_add(1, Ordering::SeqCst);

    let stopped_b = limited_use_case.open(&open_command("s-b")).unwrap();
    limited_use_case.stop(&stop_command(&stopped_b)).unwrap();
    now.fetch_add(1, Ordering::SeqCst);

    let stopped_c = limited_use_case.open(&open_command("s-c")).unwrap();
    limited_use_case.stop(&stop_command(&stopped_c)).unwrap();

    let active_snapshot = limited_use_case.recover(&active, Some(0)).unwrap();
    assert!(active_snapshot.active);
    assert!(!active_snapshot.stopped);

    let error = limited_use_case.recover(&stopped_a, Some(0)).unwrap_err();
    assert_eq!(unknown_session(&stopped_a), error);
    assert!(
        limited_use_case
            .recover(&stopped_b, Some(0))
            .unwrap()
            .stopped
    );
    assert!(
        limited_use_case
            .recover(&stopped_c, Some(0))
            .unwrap()
            .stopped
    );
}

#[test]
fn ai_off_runs_without_api_key_stt_tts_or_feedback() {
    let f = Fixture::new();
    f.session("s-off", Some(ModeType::Immediate), None, &[Q1, Q2]);
    let use_case = f.use_case();

    let voice_session_id = use_case.open(&ai_off_command("s-off")).unwrap();
    let sink = Arc::new(RecordingSink::new());
    use_case
        .register_sink(&voice_session_id, sink.clone())
        .unwrap();
    let events = sink.events();

    let ready = first_index(&events, of_type("session.ready")).unwrap();
    assert_eq!(Value::Bool(false), events[ready].1["aiEnabled"]);
    let first_prompt = first_index(&events, of_type("question.prompt")).unwrap();
    let first_speech = first_index(&events, of_type("tts.text")).unwrap();
    assert!(first_prompt > ready);
    // 질문은 기기 음성이 읽는다: 서버는 오디오가 아니라 글을 보낸다.
    assert!(first_speech > first_prompt);
    assert_eq!("question", events[first_speech].1["role"]);
    assert_eq!("question-1", events[first_speech].1["text"]);

    sink.clear();
    use_case
        .complete_answer(&answer(&voice_session_id, 1))
        .unwrap();
    let events = sink.events();

    let captured = first_index(&events, of_type("answer.captured")).unwrap();
    let question_prompt = first_index(&events, of_type("question.prompt")).unwrap();
    assert_eq!(1, events[captured].1["turnId"].as_i64().unwrap());
    assert!(question_prompt > captured);
    let next_speech = first_index(&events, of_type("tts.text")).unwrap();
    assert!(next_speech > question_prompt);
    assert_eq!("question-2", events[next_speech].1["text"]);
    assert_eq!(None, first_index(&events, of_type("stt.final")));
    assert_eq!(None, first_index(&events, of_type("tts.audio")));
    assert_eq!(None, first_index(&events, of_type("feedback.final")));
    assert!(f.stt.calls().is_empty());
    assert!(f.tts.calls().is_empty());
    assert!(f.open_ai.bootstrap_calls().is_empty());
    assert!(f.search_queries().is_empty(), "no prefetch");
    assert!(f.open_ai.generate_calls().is_empty());
    assert!(f.anthropic.generate_calls().is_empty());
}

#[test]
fn ai_off_ignores_uploaded_audio_and_duplicate_answer_signals() {
    let f = Fixture::new();
    f.session("s-off-dup", None, None, &[Q1, Q2, Q3]);
    let use_case = f.use_case();

    let voice_session_id = use_case.open(&ai_off_command("s-off-dup")).unwrap();
    let sink = Arc::new(RecordingSink::new());
    use_case
        .register_sink(&voice_session_id, sink.clone())
        .unwrap();
    sink.clear();

    use_case
        .append_audio(&audio_chunk(&voice_session_id, 5))
        .unwrap();
    assert!(sink.events().is_empty());

    use_case
        .complete_answer(&answer(&voice_session_id, 1))
        .unwrap();
    use_case
        .complete_answer(&answer(&voice_session_id, 1))
        .unwrap();

    assert_eq!(1, sink.payloads("answer.captured").len());
    assert!(f.stt.calls().is_empty());
}

#[test]
fn ai_off_stops_automatically_when_questions_are_exhausted() {
    let f = Fixture::new();
    f.session("s-off-end", Some(ModeType::Continuous), Some(3), &[Q1]);
    let use_case = f.use_case();

    let voice_session_id = use_case.open(&ai_off_command("s-off-end")).unwrap();
    let sink = Arc::new(RecordingSink::new());
    use_case
        .register_sink(&voice_session_id, sink.clone())
        .unwrap();
    sink.clear();
    use_case
        .complete_answer(&answer(&voice_session_id, 1))
        .unwrap();
    let events = sink.events();

    let stopped = first_index(&events, of_type("session.stopped")).unwrap();
    assert_eq!(
        "question_exhausted",
        events[stopped].1["reason"].as_str().unwrap().to_lowercase()
    );
    assert_eq!(None, first_index(&events, of_type("feedback.final")));
}

#[test]
fn complete_answer_is_rejected_while_ai_is_on() {
    let f = Fixture::new();
    let use_case = f.use_case();
    let voice_session_id = use_case.open(&open_command("s-on")).unwrap();

    let error = use_case
        .complete_answer(&answer(&voice_session_id, 1))
        .unwrap_err();

    assert_eq!(
        AppError::InvalidArgument(
            "This session transcribes uploaded audio; answer signals are not accepted".to_string()
        ),
        error
    );
}

#[test]
fn device_voice_request_is_treated_as_local_kokoro() {
    let f = Fixture::new();
    f.session("s-device", Some(ModeType::Immediate), None, &[Q1, Q2]);
    let use_case = f.use_case();
    let command = VoiceSessionOpenCommand {
        tts_model: Some("device-voice".to_string()),
        ..open_command("s-device")
    };

    let voice_session_id = use_case.open(&command).unwrap();
    let sink = Arc::new(RecordingSink::new());
    use_case
        .register_sink(&voice_session_id, sink.clone())
        .unwrap();
    use_case
        .append_audio(&audio_chunk(&voice_session_id, 1))
        .unwrap();

    let spoken_roles: Vec<String> = sink
        .payloads("tts.text")
        .iter()
        .map(|payload| payload["role"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(vec!["question", "feedback", "question"], spoken_roles);
    assert!(sink.payloads("tts.audio").is_empty());
    assert!(f.tts.calls().is_empty());
    assert_eq!(1, f.stt.calls().len());
}

#[test]
fn ai_off_session_opened_with_device_voice_still_works() {
    let f = Fixture::new();
    f.session("s-off-device", None, None, &[Q1]);
    let use_case = f.use_case();
    let command = VoiceSessionOpenCommand {
        tts_model: Some("device-voice".to_string()),
        ..ai_off_command("s-off-device")
    };

    let voice_session_id = use_case.open(&command).unwrap();
    let sink = Arc::new(RecordingSink::new());
    use_case
        .register_sink(&voice_session_id, sink.clone())
        .unwrap();

    assert_eq!("question-1", sink.payloads("tts.text")[0]["text"]);
    assert!(f.tts.calls().is_empty());
}

#[test]
fn local_kokoro_is_spoken_by_the_client() {
    let f = Fixture::new();
    f.session("s-kokoro", None, None, &[Q1]);
    let use_case = f.use_case();
    let command = VoiceSessionOpenCommand {
        tts_model: Some("local-kokoro".to_string()),
        ..open_command("s-kokoro")
    };

    let voice_session_id = use_case.open(&command).unwrap();
    let sink = Arc::new(RecordingSink::new());
    use_case
        .register_sink(&voice_session_id, sink.clone())
        .unwrap();

    assert_eq!("question-1", sink.payloads("tts.text")[0]["text"]);
    assert!(f.tts.calls().is_empty());
}

#[test]
fn local_stt_takes_the_transcript_from_the_client_and_never_calls_the_stt_api() {
    let f = Fixture::new();
    f.session("s-local", Some(ModeType::Immediate), None, &[Q1, Q2]);
    let use_case = f.use_case();
    let command = VoiceSessionOpenCommand {
        stt_model: Some("local-whisper".to_string()),
        ..open_command("s-local")
    };

    let voice_session_id = use_case.open(&command).unwrap();
    let sink = Arc::new(RecordingSink::new());
    use_case
        .register_sink(&voice_session_id, sink.clone())
        .unwrap();
    sink.clear();

    use_case
        .append_audio(&audio_chunk(&voice_session_id, 9))
        .unwrap();
    assert!(sink.events().is_empty());

    let answer = VoiceAnswerCommand {
        text: Some("  I went to the park yesterday.  ".to_string()),
        ..answer(&voice_session_id, 1)
    };
    use_case.complete_answer(&answer).unwrap();
    let events = sink.events();

    let stt = first_index(&events, of_type("stt.final")).unwrap();
    let feedback = first_index(&events, of_type("feedback.final")).unwrap();
    assert_eq!("I went to the park yesterday.", events[stt].1["text"]);
    assert!(feedback > stt);
    assert!(f.stt.calls().is_empty());
    assert!(Fixture::requested_feedback(
        &f.open_ai,
        "question-1",
        "I went to the park yesterday."
    ));
}

#[test]
fn local_stt_reports_an_empty_transcript_as_skipped() {
    let f = Fixture::new();
    f.session("s-local-empty", None, None, &[Q1]);
    let use_case = f.use_case();
    let command = VoiceSessionOpenCommand {
        stt_model: Some("local-whisper".to_string()),
        ..open_command("s-local-empty")
    };

    let voice_session_id = use_case.open(&command).unwrap();
    let sink = Arc::new(RecordingSink::new());
    use_case
        .register_sink(&voice_session_id, sink.clone())
        .unwrap();
    sink.clear();
    use_case
        .complete_answer(&answer(&voice_session_id, 1))
        .unwrap();
    let events = sink.events();

    assert!(first_index(&events, of_type("stt.skipped")).is_some());
    assert_eq!(None, first_index(&events, of_type("feedback.final")));
}

#[test]
fn claude_with_local_speech_opens_without_an_open_ai_key() {
    let f = Fixture::new();
    f.session("s-claude-only", Some(ModeType::Immediate), None, &[Q1, Q2]);
    let use_case = f.use_case();
    let command = VoiceSessionOpenCommand {
        session_id: Some("s-claude-only".to_string()),
        feedback_model: Some("claude-opus-5-5".to_string()),
        feedback_api_key: Some("sk-ant-key".to_string()),
        stt_model: Some("local-whisper".to_string()),
        tts_model: Some("device-voice".to_string()),
        ..VoiceSessionOpenCommand::default()
    };

    let voice_session_id = use_case.open(&command).unwrap();
    use_case
        .register_sink(&voice_session_id, Arc::new(RecordingSink::new()))
        .unwrap();
    let answer = VoiceAnswerCommand {
        text: Some("answer text".to_string()),
        ..answer(&voice_session_id, 1)
    };
    use_case.complete_answer(&answer).unwrap();

    let requests: Vec<_> = f
        .anthropic
        .generate_calls()
        .into_iter()
        .filter(|call| call.user_prompt.contains("answer text"))
        .collect();
    assert!(!requests.is_empty());
    assert!(requests.iter().all(|call| call.api_key == "sk-ant-key"));
    assert!(f.stt.calls().is_empty());
    assert!(f.tts.calls().is_empty());
}

#[test]
fn claude_model_uses_feedback_api_key_and_anthropic_provider_for_feedback() {
    let f = Fixture::new();
    f.session("s-claude", Some(ModeType::Immediate), None, &[Q1, Q2]);
    let use_case = f.use_case();
    let command = VoiceSessionOpenCommand {
        feedback_model: Some("claude-opus-5-5".to_string()),
        feedback_api_key: Some("sk-ant-key".to_string()),
        ..open_command("s-claude")
    };

    let voice_session_id = use_case.open(&command).unwrap();
    use_case
        .register_sink(&voice_session_id, Arc::new(RecordingSink::new()))
        .unwrap();
    use_case
        .append_audio(&audio_chunk(&voice_session_id, 1))
        .unwrap();

    // provider "anthropic": 피드백은 Anthropic 클라이언트가, 모델 claude-opus-5-5와 피드백 키로.
    assert!(Fixture::requested_feedback(
        &f.anthropic,
        "question-1",
        "answer text"
    ));
    assert!(f
        .anthropic
        .generate_calls()
        .iter()
        .all(|call| call.api_key == "sk-ant-key" && call.model == "claude-opus-5-5"));
    assert!(f.open_ai.generate_calls().is_empty());
    let stt_calls = f.stt.calls();
    assert_eq!(1, stt_calls.len());
    assert_eq!("api-key", stt_calls[0].api_key);
    let tts_calls = f.tts.calls();
    assert_eq!(3, tts_calls.len());
    assert!(tts_calls.iter().all(|call| call.api_key == "api-key"));
}

#[test]
fn open_throws_when_claude_model_has_no_feedback_api_key() {
    let f = Fixture::new();
    let use_case = f.use_case();
    let command = VoiceSessionOpenCommand {
        feedback_model: Some("claude-sonnet-5-5".to_string()),
        ..open_command("s-claude-nokey")
    };

    let error = use_case.open(&command).unwrap_err();

    assert_eq!(
        AppError::InvalidArgument(
            "Anthropic API key is required for Claude feedback models".to_string()
        ),
        error
    );
}

// ---- Java에 없는 테스트 ----

#[test]
fn sink_send_false_stops_emitting_and_keeps_session_recoverable() {
    let f = Fixture::new();
    f.session("s1", Some(ModeType::Immediate), None, &[Q1, Q2, Q3]);
    let use_case = f.use_case();
    let voice_session_id = use_case.open(&open_command("s1")).unwrap();
    let sink = Arc::new(RecordingSink::new());
    use_case
        .register_sink(&voice_session_id, sink.clone())
        .unwrap();
    let seen = sink.events();
    let last_seen = event_id(&seen.last().unwrap().1);

    sink.close();
    use_case
        .append_audio(&audio_chunk(&voice_session_id, 1))
        .unwrap();
    use_case
        .append_audio(&audio_chunk(&voice_session_id, 2))
        .unwrap();

    assert_eq!(seen, sink.events());
    let snapshot: RecoverySnapshot = use_case
        .recover(&voice_session_id, Some(last_seen))
        .unwrap();
    assert!(snapshot.active);
    assert!(!snapshot.stopped);
    assert_eq!(3, snapshot.current_turn_id);
    assert_eq!(
        Some("q-3".to_string()),
        snapshot.current_question.unwrap().id
    );
    assert_eq!(Some(2), snapshot.last_accepted_chunk_sequence);
    assert!(!snapshot.turn_processing);
    assert!(snapshot.latest_event_id > last_seen);
    assert_eq!(last_seen, snapshot.replay_from_event_id);
    assert!(!snapshot.gap_detected);
    assert_eq!(2, f.stt.calls().len());

    // 다시 붙은 싱크는 놓친 이벤트를 모두 받는다.
    let resumed = Arc::new(RecordingSink::new());
    use_case
        .register_sink_since(&voice_session_id, resumed.clone(), Some(last_seen))
        .unwrap();
    let resumed_ids: Vec<i64> = resumed.events().iter().map(|(_, p)| event_id(p)).collect();
    assert_eq!(
        ((last_seen + 1)..=snapshot.latest_event_id).collect::<Vec<_>>(),
        resumed_ids
    );
}

#[test]
fn unregister_sink_stops_delivery() {
    let f = Fixture::new();
    f.session("s1", Some(ModeType::Immediate), None, &[Q1, Q2]);
    let use_case = f.use_case();
    let voice_session_id = use_case.open(&open_command("s1")).unwrap();
    let sink = Arc::new(RecordingSink::new());
    let handle: Arc<dyn crate::voice::model::VoiceEventSink> = sink.clone();
    use_case
        .register_sink(&voice_session_id, handle.clone())
        .unwrap();
    let before = sink.events();

    use_case.unregister_sink(&voice_session_id, &handle);
    use_case.unregister_sink("unknown", &handle);
    use_case
        .append_audio(&audio_chunk(&voice_session_id, 1))
        .unwrap();

    assert_eq!(before, sink.events());
    assert_eq!(1, f.stt.calls().len());
}

#[test]
fn feedback_searches_rulebooks_itself_when_prefetch_has_not_finished() {
    let executor = Arc::new(DeferredExecutor::new());
    let f = Fixture::with(java_voice_policy(), executor.clone());
    f.session("s1", Some(ModeType::Immediate), None, &[Q1, Q2]);
    let use_case = f.use_case();
    let voice_session_id = use_case.open(&open_command("s1")).unwrap();
    let sink = Arc::new(RecordingSink::new());
    use_case
        .register_sink(&voice_session_id, sink.clone())
        .unwrap();
    assert!(executor.run_next(), "initialize");
    assert_eq!(1, executor.pending(), "prefetch for q-1 is queued");

    use_case
        .append_audio(&audio_chunk(&voice_session_id, 1))
        .unwrap();
    assert!(
        use_case
            .recover(&voice_session_id, None)
            .unwrap()
            .turn_processing
    );
    // 턴이 프리페치보다 먼저 돈다.
    assert!(executor.run_last());
    assert!(
        !use_case
            .recover(&voice_session_id, None)
            .unwrap()
            .turn_processing
    );

    assert_eq!(
        vec!["question-1\nanswer text".to_string()],
        f.search_queries()[..1].to_vec()
    );
    assert_eq!(1, sink.payloads("feedback.final").len());
    // 늦게 끝난 q-1 프리페치는 현재 질문(q-2)의 것이 아니므로 버려진다.
    assert_eq!(2, executor.run_all());
    assert_eq!(
        vec!["question-1\nanswer text", "question-1", "question-2"],
        f.search_queries()
    );
}

#[test]
fn audio_arriving_during_a_turn_is_processed_after_it() {
    let executor = Arc::new(DeferredExecutor::new());
    let f = Fixture::with(java_voice_policy(), executor.clone());
    f.session("s1", Some(ModeType::Immediate), None, &[Q1, Q2, Q3]);
    let use_case = f.use_case();
    let voice_session_id = use_case.open(&open_command("s1")).unwrap();
    let sink = Arc::new(RecordingSink::new());
    use_case
        .register_sink(&voice_session_id, sink.clone())
        .unwrap();
    executor.run_all();

    use_case
        .append_audio(&audio_chunk(&voice_session_id, 1))
        .unwrap();
    use_case
        .append_audio(&audio_chunk(&voice_session_id, 2))
        .unwrap();
    let snapshot = use_case.recover(&voice_session_id, None).unwrap();
    assert!(snapshot.turn_processing);
    assert!(snapshot.has_buffered_audio);
    assert_eq!(1, executor.pending(), "only one turn runs at a time");

    executor.run_all();

    let finals = sink.payloads("stt.final");
    assert_eq!(2, finals.len());
    assert_eq!(1, finals[0]["turnId"]);
    assert_eq!(2, finals[1]["turnId"]);
    let snapshot = use_case.recover(&voice_session_id, None).unwrap();
    assert!(!snapshot.turn_processing);
    assert!(!snapshot.has_buffered_audio);
    assert_eq!(3, snapshot.current_turn_id);
}

#[test]
fn continuous_mode_emits_residual_feedback_when_questions_run_out() {
    let f = Fixture::new();
    f.session("s1", Some(ModeType::Continuous), Some(3), &[Q1, Q2]);
    let use_case = f.use_case();
    let voice_session_id = use_case.open(&open_command("s1")).unwrap();
    let sink = Arc::new(RecordingSink::new());
    use_case
        .register_sink(&voice_session_id, sink.clone())
        .unwrap();

    use_case
        .append_audio(&audio_chunk(&voice_session_id, 1))
        .unwrap();
    assert!(
        sink.payloads("feedback.final").is_empty(),
        "batch of 3 is not full"
    );
    use_case
        .append_audio(&audio_chunk(&voice_session_id, 2))
        .unwrap();

    let feedback = sink.payloads("feedback.final");
    assert_eq!(1, feedback.len());
    let payload = &feedback[0];
    assert_eq!(2, payload["turnId"]);
    assert_eq!(
        serde_json::json!({"mode": "CONTINUOUS", "groupBatchSize": 3, "reason": "EXHAUSTED_WITH_REMAINDER"}),
        payload["policy"]
    );
    assert_eq!(2, payload["batch"]["size"]);
    assert_eq!(true, payload["batch"]["isResidual"]);
    let items = payload["batch"]["items"].as_array().unwrap();
    assert_eq!("q-1", items[0]["questionId"]);
    assert_eq!("travel", items[0]["questionGroup"]);
    assert_eq!("q-2", items[1]["questionId"]);
    assert_eq!(
        serde_json::json!({"type": "auto_stop", "reason": "QUESTION_EXHAUSTED"}),
        payload["nextAction"]
    );
    let names = sink.event_names();
    let feedback_index = names.iter().position(|n| n == "feedback.final").unwrap();
    let stopped_index = names.iter().position(|n| n == "session.stopped").unwrap();
    assert!(feedback_index < stopped_index);
    let prompts = sink.payloads("question.prompt");
    let last_prompt = prompts.last().unwrap();
    assert_eq!(Value::Null, last_prompt["question"]);
    assert_eq!(2, last_prompt["turnId"]);
    assert_eq!(
        serde_json::json!({"mode": "CONTINUOUS", "exhausted": true, "reason": "QUESTION_EXHAUSTED"}),
        last_prompt["selection"]
    );
}

#[test]
fn event_payloads_match_java_shapes() {
    let f = Fixture::new();
    f.session("s1", Some(ModeType::Immediate), None, &[Q1]);
    let use_case = f.use_case();
    let voice_session_id = use_case.open(&open_command("s1")).unwrap();
    let sink = Arc::new(RecordingSink::new());
    use_case
        .register_sink(&voice_session_id, sink.clone())
        .unwrap();
    use_case
        .append_audio(&audio_chunk(&voice_session_id, 1))
        .unwrap();

    let ready = &sink.payloads("session.ready")[0];
    assert_eq!(
        serde_json::json!({
            "sessionId": "s1",
            "voiceSessionId": voice_session_id,
            "aiEnabled": true,
            "feedbackProvider": "openai",
            "eventId": 1,
        }),
        *ready
    );
    assert_eq!(
        serde_json::json!({
            "sessionId": "s1",
            "turnId": 1,
            "question": {"id": "q-1", "text": "question-1", "group": "travel", "groupId": "g-1", "questionType": "OPEN"},
            "selection": {"mode": "IMMEDIATE", "exhausted": false},
            "eventId": 2,
        }),
        sink.payloads("question.prompt")[0]
    );
    let speech = &sink.payloads("tts.audio")[0];
    assert_eq!(
        serde_json::json!({
            "sessionId": "s1",
            "turnId": 1,
            "role": "question",
            "phase": "question",
            "sequence": 1,
            "text": "question-1",
            "audio": "AQIDBA==",
            "mimeType": "audio/wav",
            "eventId": 3,
        }),
        *speech
    );
    let stt_final = &sink.payloads("stt.final")[0];
    assert_eq!(
        serde_json::json!({"sessionId": "s1", "turnId": 1, "text": "answer text", "eventId": 4}),
        *stt_final
    );
    let feedback = &sink.payloads("feedback.final")[0];
    assert_eq!(
        serde_json::json!({"mode": "IMMEDIATE", "groupBatchSize": 1, "reason": "IMMEDIATE_MODE"}),
        feedback["policy"]
    );
    assert_eq!(false, feedback["batch"]["isResidual"]);
    assert_eq!("answer text", feedback["batch"]["items"][0]["answerText"]);
    assert_eq!(
        serde_json::json!({"type": "auto_stop", "reason": "QUESTION_EXHAUSTED"}),
        feedback["nextAction"]
    );
    let stopped = sink.payloads("session.stopped");
    assert_eq!(1, stopped.len());
    let mut stopped = stopped[0].clone();
    stopped.as_object_mut().unwrap().remove("eventId");
    assert_eq!(
        serde_json::json!({
            "sessionId": "s1",
            "voiceSessionId": voice_session_id,
            "forced": false,
            "reason": "QUESTION_EXHAUSTED",
        }),
        stopped
    );
    let snapshot = use_case.recover(&voice_session_id, None).unwrap();
    assert!(snapshot.stopped);
    assert_eq!("QUESTION_EXHAUSTED", snapshot.stop_reason);
    assert_eq!(None, snapshot.current_question);
}

#[test]
fn tts_failure_emits_tts_error_and_keeps_going() {
    let f = Fixture::new();
    f.session("s1", Some(ModeType::Immediate), None, &[Q1, Q2]);
    f.tts.fail_with(AppError::Internal("tts down".to_string()));
    let use_case = f.use_case();
    let voice_session_id = use_case.open(&open_command("s1")).unwrap();
    let sink = Arc::new(RecordingSink::new());
    use_case
        .register_sink(&voice_session_id, sink.clone())
        .unwrap();

    assert_eq!(
        serde_json::json!({"sessionId": "s1", "turnId": 1, "message": "tts down", "eventId": 3}),
        sink.payloads("tts.error")[0]
    );
    use_case
        .append_audio(&audio_chunk(&voice_session_id, 1))
        .unwrap();
    assert_eq!(1, sink.payloads("feedback.final").len());
    assert_eq!(3, sink.payloads("tts.error").len());
}

#[test]
fn commands_are_validated_with_java_messages() {
    let f = Fixture::new();
    let use_case = f.use_case();
    let invalid = |message: &str| AppError::InvalidArgument(message.to_string());
    let voice_session_id = use_case.open(&open_command("s1")).unwrap();

    assert_eq!(
        invalid("sessionId is required"),
        use_case
            .open(&VoiceSessionOpenCommand::default())
            .unwrap_err()
    );
    assert_eq!(
        invalid("voiceSessionId is required"),
        use_case
            .append_audio(&VoiceAudioChunkCommand::default())
            .unwrap_err()
    );
    let no_audio = VoiceAudioChunkCommand {
        pcm16_base64: Some(" ".to_string()),
        ..audio_chunk(&voice_session_id, 1)
    };
    assert_eq!(
        invalid("pcm16Base64 is required"),
        use_case.append_audio(&no_audio).unwrap_err()
    );
    let bad_audio = VoiceAudioChunkCommand {
        pcm16_base64: Some("not base64!".to_string()),
        ..audio_chunk(&voice_session_id, 2)
    };
    assert_eq!(
        invalid("Invalid base64 audio chunk"),
        use_case.append_audio(&bad_audio).unwrap_err()
    );
    assert_eq!(
        unknown_session("missing"),
        use_case
            .append_audio(&audio_chunk("missing", 1))
            .unwrap_err()
    );
    assert_eq!(
        invalid("voiceSessionId is required"),
        use_case
            .complete_answer(&VoiceAnswerCommand::default())
            .unwrap_err()
    );
    assert_eq!(
        invalid("voiceSessionId is required"),
        use_case
            .stop(&VoiceSessionStopCommand::default())
            .unwrap_err()
    );
    assert_eq!(Ok(()), use_case.stop(&stop_command("missing")));
    assert_eq!(
        unknown_session("missing"),
        use_case
            .register_sink("missing", Arc::new(RecordingSink::new()))
            .unwrap_err()
    );
}

#[test]
fn user_stop_is_reported_once_with_reason() {
    let f = Fixture::new();
    f.session("s1", Some(ModeType::Immediate), None, &[Q1, Q2]);
    let use_case = f.use_case();
    let voice_session_id = use_case.open(&open_command("s1")).unwrap();
    let sink = Arc::new(RecordingSink::new());
    use_case
        .register_sink(&voice_session_id, sink.clone())
        .unwrap();

    use_case.stop(&stop_command(&voice_session_id)).unwrap();
    use_case
        .stop(&VoiceSessionStopCommand {
            reason: Some("again".to_string()),
            ..stop_command(&voice_session_id)
        })
        .unwrap();
    use_case
        .append_audio(&audio_chunk(&voice_session_id, 1))
        .unwrap();

    let stopped = sink.payloads("session.stopped");
    assert_eq!(1, stopped.len());
    assert_eq!("user_stop", stopped[0]["reason"]);
    assert_eq!(true, stopped[0]["forced"]);
    assert!(f.stt.calls().is_empty());
    let snapshot = use_case.recover(&voice_session_id, None).unwrap();
    assert!(snapshot.stopped);
    assert_eq!("user_stop", snapshot.stop_reason);
}

/// 첫 호출은 패닉하고 그 뒤로는 "answer text"를 돌려주는 STT.
#[derive(Default)]
struct PanicOnceStt {
    calls: AtomicUsize,
}

impl SttGateway for PanicOnceStt {
    fn transcribe(
        &self,
        _file_bytes: &[u8],
        _model: &str,
        _api_key: &str,
        _translate: bool,
        _vad_settings: &VadSettings,
    ) -> omypic_domain::error::AppResult<String> {
        if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
            panic!("stt gateway exploded");
        }
        Ok("answer text".to_string())
    }
}

#[test]
fn panic_inside_an_audio_turn_reports_session_error_and_accepts_the_next_answer() {
    let mut f = Fixture::new();
    f.deps.stt_use_case = Arc::new(SttUseCase::new(
        Arc::new(PanicOnceStt::default()),
        Arc::new(FakeSttPolicy::default()),
        Arc::new(RecordingMetrics::new()),
    ));
    f.session("s1", Some(ModeType::Immediate), None, &[Q1, Q2, Q3]);
    let use_case = f.use_case();
    let voice_session_id = use_case.open(&open_command("s1")).unwrap();
    let sink = Arc::new(RecordingSink::new());
    use_case
        .register_sink(&voice_session_id, sink.clone())
        .unwrap();
    sink.clear();

    use_case
        .append_audio(&audio_chunk(&voice_session_id, 1))
        .unwrap();

    let snapshot = use_case.recover(&voice_session_id, None).unwrap();
    assert!(snapshot.active);
    assert!(!snapshot.turn_processing);
    let errors = sink.payloads("session.error");
    assert_eq!(1, errors.len(), "{:?}", sink.events());
    assert_eq!(1, errors[0]["turnId"]);

    use_case
        .append_audio(&audio_chunk(&voice_session_id, 2))
        .unwrap();

    let finals = sink.payloads("stt.final");
    assert_eq!(1, finals.len(), "{:?}", sink.events());
    assert_eq!(1, sink.payloads("feedback.final").len());
    assert!(
        !use_case
            .recover(&voice_session_id, None)
            .unwrap()
            .turn_processing
    );
}

#[test]
fn panic_inside_a_transcript_turn_reports_session_error_and_accepts_the_next_answer() {
    let f = Fixture::new();
    f.session("s-local", Some(ModeType::Immediate), None, &[Q1, Q2, Q3]);
    let use_case = f.use_case();
    let command = VoiceSessionOpenCommand {
        stt_model: Some("local-whisper".to_string()),
        ..open_command("s-local")
    };
    let voice_session_id = use_case.open(&command).unwrap();
    let sink = Arc::new(RecordingSink::new());
    use_case
        .register_sink(&voice_session_id, sink.clone())
        .unwrap();
    sink.clear();
    f.feedback_panic.store(true, Ordering::SeqCst);

    let first = VoiceAnswerCommand {
        text: Some("first answer".to_string()),
        ..answer(&voice_session_id, 1)
    };
    use_case.complete_answer(&first).unwrap();

    assert!(
        !use_case
            .recover(&voice_session_id, None)
            .unwrap()
            .turn_processing
    );
    assert!(
        !sink.payloads("session.error").is_empty(),
        "{:?}",
        sink.events()
    );

    let second = VoiceAnswerCommand {
        text: Some("second answer".to_string()),
        ..answer(&voice_session_id, 2)
    };
    use_case.complete_answer(&second).unwrap();
    assert!(Fixture::requested_feedback(&f.open_ai, "", "second answer"));
    assert!(
        !use_case
            .recover(&voice_session_id, None)
            .unwrap()
            .turn_processing
    );
}

#[test]
fn turn_in_flight_when_the_session_stops_does_no_more_work() {
    let executor = Arc::new(DeferredExecutor::new());
    let f = Fixture::with(java_voice_policy(), executor.clone());
    f.session("s1", Some(ModeType::Immediate), None, &[Q1, Q2, Q3]);
    let use_case = f.use_case();
    let voice_session_id = use_case.open(&open_command("s1")).unwrap();
    let sink = Arc::new(RecordingSink::new());
    use_case
        .register_sink(&voice_session_id, sink.clone())
        .unwrap();
    executor.run_all();
    let generate_calls_before = f.open_ai.generate_calls().len();
    use_case
        .append_audio(&audio_chunk(&voice_session_id, 1))
        .unwrap();
    assert_eq!(1, executor.pending(), "the turn is queued");

    // 사용자가 정지한 뒤에 STT를 마친 턴이 돈다.
    use_case.stop(&stop_command(&voice_session_id)).unwrap();
    sink.clear();
    executor.run_all();

    let events = sink.events();
    for name in [
        "stt.final",
        "feedback.final",
        "question.prompt",
        "tts.audio",
    ] {
        assert_eq!(
            None,
            first_index(&events, of_type(name)),
            "{name}: {events:?}"
        );
    }
    assert_eq!(generate_calls_before, f.open_ai.generate_calls().len());
    // 다음 질문을 꺼내지 않았으므로 세션의 다음 질문은 그대로 question-2다.
    let next = f.deps.question_use_case.next_question("s1").unwrap();
    assert_eq!(Some("question-2"), next.text.as_deref());
    assert!(
        !use_case
            .recover(&voice_session_id, None)
            .unwrap()
            .turn_processing
    );
}
