//! 버려진 음성 세션의 수명: 이벤트 스트림이 붙어 있지 않고 유예 시간 동안 아무 일도 없으면
//! 세션을 멈추고, 정지 세션은 보관 기간이 지나면 지운다. 유예 시간 안에 돌아온 클라이언트는
//! 그대로 이어 간다.

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

use omypic_domain::session::ModeType;

use super::tests::{
    ai_off_command, answer, audio_chunk, java_voice_policy, open_command, unknown_session, Fixture,
    Q1, Q2,
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
