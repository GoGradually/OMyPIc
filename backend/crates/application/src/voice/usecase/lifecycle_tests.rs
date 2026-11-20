//! 버려진 음성 세션의 수명: 이벤트 스트림이 붙어 있지 않고 유예 시간 동안 아무 일도 없으면
//! 세션을 멈추고, 정지 세션은 보관 기간이 지나면 지운다. 유예 시간 안에 돌아온 클라이언트는
//! 그대로 이어 간다.

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

use omypic_domain::session::ModeType;

use super::tests::{
    ai_off_command, answer, audio_chunk, java_voice_policy, open_command, stop_command,
    unknown_session, Fixture, Q1, Q2,
};
use super::VoiceSessionUseCase;
use crate::test_support::{DeferredExecutor, FakeVoicePolicy, ImmediateExecutor, RecordingSink};
use crate::voice::model::VoiceEventSink;

const IDLE_MS: i64 = 60_000;
const RETENTION_MS: i64 = 120_000;

struct Clocked {
    _fixture: Fixture,
    now: Arc<AtomicI64>,
    use_case: VoiceSessionUseCase,
}

impl Clocked {
    fn advance(&self, ms: i64) {
        self.now.fetch_add(ms, Ordering::SeqCst);
    }
}

fn policy() -> FakeVoicePolicy {
    FakeVoicePolicy {
        idle_timeout_ms: IDLE_MS,
        recovery_retention_ms: RETENTION_MS,
        ..java_voice_policy()
    }
}

fn clocked_with(fixture: Fixture) -> Clocked {
    fixture.session("s1", Some(ModeType::Immediate), None, &[Q1, Q2]);
    let now = Arc::new(AtomicI64::new(1_000_000));
    let clock = now.clone();
    let use_case = VoiceSessionUseCase::with_clock(
        fixture.deps.clone(),
        256,
        Arc::new(move || clock.load(Ordering::SeqCst)),
    );
    Clocked {
        _fixture: fixture,
        now,
        use_case,
    }
}

fn clocked() -> Clocked {
    clocked_with(Fixture::with(policy(), Arc::new(ImmediateExecutor)))
}

fn attach(c: &Clocked, voice_session_id: &str) -> Arc<dyn VoiceEventSink> {
    let sink: Arc<dyn VoiceEventSink> = Arc::new(RecordingSink::new());
    c.use_case
        .register_sink(voice_session_id, sink.clone())
        .unwrap();
    sink
}

#[test]
fn session_whose_event_stream_went_away_is_stopped_after_the_idle_timeout() {
    let c = clocked();
    let id = c.use_case.open(&ai_off_command("s1")).unwrap();
    let sink = attach(&c, &id);
    c.use_case.unregister_sink(&id, &sink);

    c.advance(IDLE_MS - 1);
    c.use_case.reap_idle_sessions();
    let snapshot = c.use_case.recover(&id, Some(0)).unwrap();
    assert!(snapshot.active, "still inside the grace period");

    // recover above was activity; wait the whole grace period again without any call.
    c.advance(IDLE_MS);
    c.use_case.reap_idle_sessions();
    let snapshot = c.use_case.recover(&id, Some(0)).unwrap();
    assert!(snapshot.stopped);
    assert_eq!("idle_timeout", snapshot.stop_reason);
}

#[test]
fn reaped_session_is_removed_after_the_recovery_retention() {
    let c = clocked();
    let id = c.use_case.open(&ai_off_command("s1")).unwrap();

    c.advance(IDLE_MS);
    c.use_case.reap_idle_sessions();
    c.advance(RETENTION_MS);
    c.use_case.reap_idle_sessions();

    assert_eq!(
        unknown_session(&id),
        c.use_case.recover(&id, Some(0)).unwrap_err()
    );
}

#[test]
fn opened_session_that_never_attaches_an_event_stream_is_reaped() {
    let c = clocked();
    let id = c.use_case.open(&ai_off_command("s1")).unwrap();

    c.advance(IDLE_MS);
    c.use_case.reap_idle_sessions();

    let snapshot = c.use_case.recover(&id, Some(0)).unwrap();
    assert!(snapshot.stopped);
    assert_eq!("idle_timeout", snapshot.stop_reason);
}

#[test]
fn session_with_an_attached_event_stream_is_never_reaped() {
    let c = clocked();
    let id = c.use_case.open(&ai_off_command("s1")).unwrap();
    let _sink = attach(&c, &id);

    c.advance(IDLE_MS * 100);
    c.use_case.reap_idle_sessions();

    assert!(c.use_case.recover(&id, Some(0)).unwrap().active);
}

#[test]
fn client_that_reconnects_within_the_grace_period_keeps_its_session() {
    let c = clocked();
    let id = c.use_case.open(&ai_off_command("s1")).unwrap();
    let first = attach(&c, &id);
    c.use_case.unregister_sink(&id, &first);

    c.advance(IDLE_MS / 2);
    let latest = c.use_case.recover(&id, Some(0)).unwrap().latest_event_id;
    let second: Arc<dyn VoiceEventSink> = Arc::new(RecordingSink::new());
    c.use_case
        .register_sink_since(&id, second.clone(), Some(latest))
        .unwrap();
    c.advance(IDLE_MS * 3);
    c.use_case.reap_idle_sessions();
    c.use_case.complete_answer(&answer(&id, 1)).unwrap();

    // The second stream drops; the grace period starts again from that moment.
    c.use_case.unregister_sink(&id, &second);
    c.advance(IDLE_MS - 1);
    c.use_case.reap_idle_sessions();
    assert!(c.use_case.recover(&id, Some(0)).unwrap().active);
}

#[test]
fn turn_in_progress_is_not_reaped() {
    let executor = Arc::new(DeferredExecutor::new());
    let c = clocked_with(Fixture::with(policy(), executor.clone()));
    let idle = c.use_case.open(&ai_off_command("s1")).unwrap();
    let busy = c.use_case.open(&open_command("s1")).unwrap();
    let busy_sink = attach(&c, &busy);
    while executor.run_next() {}
    // The answer's turn task is queued but has not finished (a slow STT or LLM call).
    c.use_case.append_audio(&audio_chunk(&busy, 1)).unwrap();
    c.use_case.unregister_sink(&busy, &busy_sink);
    assert!(executor.pending() > 0, "turn should still be queued");

    c.advance(IDLE_MS * 2);
    c.use_case.reap_idle_sessions();

    assert!(c.use_case.recover(&busy, Some(0)).unwrap().active);
    assert!(c.use_case.recover(&idle, Some(0)).unwrap().stopped);
}

#[test]
fn sink_whose_stream_closed_without_unregistering_does_not_keep_the_session_alive() {
    // An SSE request dropped while its registration was still running registers a sink that
    // nobody unregisters; no event is emitted afterwards to notice that it is dead.
    let c = clocked();
    let id = c.use_case.open(&ai_off_command("s1")).unwrap();
    let sink = Arc::new(RecordingSink::new());
    c.use_case.register_sink(&id, sink.clone()).unwrap();
    sink.close();

    c.advance(IDLE_MS);
    c.use_case.reap_idle_sessions();

    let snapshot = c.use_case.recover(&id, Some(0)).unwrap();
    assert!(snapshot.stopped);
    assert_eq!("idle_timeout", snapshot.stop_reason);
}

#[test]
fn a_stream_attached_to_a_session_the_reaper_just_stopped_receives_session_stopped() {
    let c = clocked();
    let id = c.use_case.open(&ai_off_command("s1")).unwrap();
    let first = attach(&c, &id);
    c.use_case.unregister_sink(&id, &first);
    c.advance(IDLE_MS);
    c.use_case.reap_idle_sessions();

    // The client comes back without a since id (a fresh EventSource).
    let late = Arc::new(RecordingSink::new());
    c.use_case.register_sink(&id, late.clone()).unwrap();

    assert_eq!(vec!["session.stopped".to_string()], late.event_names());
    assert_eq!(
        "idle_timeout",
        late.payloads("session.stopped")[0]["reason"]
    );
}

/// 이 스레드에서 `needle`을 담은 로그가 처음 나올 때 그 자리에서 `hook`을 부른다.
fn on_first_log<R>(
    needle: &'static str,
    hook: impl FnOnce() + Send + 'static,
    f: impl FnOnce() -> R,
) -> R {
    use std::sync::Mutex;

    type Hook = Arc<Mutex<Option<Box<dyn FnOnce() + Send>>>>;
    struct HookWriter {
        hook: Hook,
        needle: &'static str,
    }
    impl std::io::Write for HookWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if String::from_utf8_lossy(bytes).contains(self.needle) {
                let hook = self.hook.lock().unwrap().take();
                if let Some(hook) = hook {
                    hook();
                }
            }
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let hook: Hook = Arc::new(Mutex::new(Some(Box::new(hook))));
    let subscriber = tracing_subscriber::fmt()
        .with_writer(move || HookWriter {
            hook: Arc::clone(&hook),
            needle,
        })
        .with_ansi(false)
        .finish();
    tracing::subscriber::with_default(subscriber, f)
}

#[test]
fn a_client_that_reattaches_while_the_reaper_decides_is_not_stopped_on_its_new_stream() {
    let c = Arc::new(clocked());
    let id = c.use_case.open(&ai_off_command("s1")).unwrap();
    let first = attach(&c, &id);
    c.use_case.unregister_sink(&id, &first);
    c.advance(IDLE_MS);

    // The reaper logs right after it has decided to stop the session. A client reattaching at
    // that moment must either keep a running session or be told that it stopped.
    let reattached = Arc::new(RecordingSink::new());
    let was_running = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let hook = {
        let (c, id, reattached, was_running) = (
            Arc::clone(&c),
            id.clone(),
            Arc::clone(&reattached),
            Arc::clone(&was_running),
        );
        move || {
            // Every call first runs the same cleanup. Seen from that nested cleanup the client
            // has just been active, as it would be after reconnecting a moment ago.
            c.now.fetch_sub(IDLE_MS, Ordering::SeqCst);
            c.use_case
                .register_sink_since(&id, reattached, Some(0))
                .unwrap();
            let snapshot = c.use_case.recover(&id, None).unwrap();
            was_running.store(snapshot.active, Ordering::SeqCst);
        }
    };
    on_first_log("Stopping voice session", hook, || {
        c.use_case.reap_idle_sessions()
    });

    let snapshot = c.use_case.recover(&id, Some(0)).unwrap();
    if was_running.load(Ordering::SeqCst) {
        assert!(
            snapshot.active,
            "the reattached session was stopped by the reaper on its new stream"
        );
    } else {
        assert!(snapshot.stopped);
        assert!(reattached
            .event_names()
            .contains(&"session.stopped".to_string()));
    }
}

/// 24kHz 16비트 모노 10초(약 480KB, base64로 약 640KB). 서버 TTS가 질문 하나에 돌려주는 크기.
fn realistic_tts_wav() -> Vec<u8> {
    vec![0x5a; 480_044]
}

#[test]
fn a_stopped_session_keeps_no_tts_audio_in_its_replay_buffer() {
    let fixture =
        Fixture::with_tts_audio(policy(), Arc::new(ImmediateExecutor), &realistic_tts_wav());
    let c = clocked_with(fixture);
    let id = c.use_case.open(&open_command("s1")).unwrap();
    let sink = Arc::new(RecordingSink::new());
    c.use_case.register_sink(&id, sink.clone()).unwrap();
    c.use_case.append_audio(&audio_chunk(&id, 1)).unwrap();
    let running = c.use_case.replay_buffer_bytes(&id).unwrap();
    let audio_events = sink.payloads("tts.audio").len();

    c.use_case.stop(&stop_command(&id)).unwrap();

    let stopped = c.use_case.replay_buffer_bytes(&id).unwrap();
    println!(
        "replay buffer: running {running} bytes ({audio_events} tts.audio), stopped {stopped} bytes"
    );
    assert!(audio_events >= 2, "{audio_events}");
    assert!(
        stopped < 16 * 1024,
        "a stopped session keeps {stopped} bytes"
    );
    // The skeleton stays: recovery still knows every event id.
    let snapshot = c.use_case.recover(&id, Some(0)).unwrap();
    assert!(!snapshot.gap_detected);
    let replayed = Arc::new(RecordingSink::new());
    c.use_case
        .register_sink_since(&id, replayed.clone(), Some(0))
        .unwrap();
    assert_eq!(sink.event_names(), replayed.event_names());
    assert!(replayed
        .payloads("tts.audio")
        .iter()
        .all(|p| p.get("audio").is_none()));
}

#[test]
fn a_running_session_bounds_its_replay_buffer_by_bytes_keeping_the_newest_audio() {
    let fixture =
        Fixture::with_tts_audio(policy(), Arc::new(ImmediateExecutor), &realistic_tts_wav());
    let questions: Vec<super::tests::Question> = (0..12)
        .map(|n| {
            let leak = |text: String| -> &'static str { Box::leak(text.into_boxed_str()) };
            (
                leak(format!("q-{n}")),
                leak(format!("question-{n}")),
                leak(format!("g-{n}")),
                leak(format!("group-{n}")),
            )
        })
        .collect();
    let c = clocked_with(fixture);
    c._fixture
        .session("long", Some(ModeType::Immediate), None, &questions);
    let id = c.use_case.open(&open_command("long")).unwrap();
    let sink = Arc::new(RecordingSink::new());
    c.use_case.register_sink(&id, sink.clone()).unwrap();
    for sequence in 1..=6 {
        c.use_case
            .append_audio(&audio_chunk(&id, sequence))
            .unwrap();
    }

    let bytes = c.use_case.replay_buffer_bytes(&id).unwrap();
    let sent = sink.payloads("tts.audio").len();
    println!("running replay buffer after {sent} tts.audio events: {bytes} bytes");
    assert!(c.use_case.recover(&id, Some(0)).unwrap().active);
    assert!(sent >= 10, "{sent}");
    assert!(bytes <= 5 * 1024 * 1024, "{bytes} bytes");
    let replayed = Arc::new(RecordingSink::new());
    c.use_case
        .register_sink_since(&id, replayed.clone(), Some(0))
        .unwrap();
    let audio = replayed.payloads("tts.audio");
    assert_eq!(sent, audio.len(), "every event id is still replayed");
    assert!(
        audio.last().unwrap().get("audio").is_some(),
        "the newest audio is kept"
    );
    assert!(!c.use_case.recover(&id, Some(0)).unwrap().gap_detected);
}

#[test]
fn the_system_clock_never_goes_back_and_starts_at_the_wall_clock() {
    let clock = super::system_now_millis();
    let wall = chrono::Utc::now().timestamp_millis();
    let first = clock();
    assert!((first - wall).abs() < 60_000, "first={first} wall={wall}");
    let mut previous = first;
    for _ in 0..10_000 {
        let now = clock();
        assert!(now >= previous);
        previous = now;
    }
    // Another clock made later reads the same time line.
    assert!(super::system_now_millis()() >= previous);
}

#[test]
fn a_zero_or_negative_idle_timeout_means_the_default() {
    use super::{effective_voice_idle_timeout_ms, DEFAULT_VOICE_IDLE_TIMEOUT_MS};
    assert_eq!(
        DEFAULT_VOICE_IDLE_TIMEOUT_MS,
        effective_voice_idle_timeout_ms(0)
    );
    assert_eq!(
        DEFAULT_VOICE_IDLE_TIMEOUT_MS,
        effective_voice_idle_timeout_ms(-5)
    );
    assert_eq!(1_000, effective_voice_idle_timeout_ms(1_000));
}
