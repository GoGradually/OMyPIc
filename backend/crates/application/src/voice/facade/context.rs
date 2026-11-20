//! 음성 세션 하나의 런타임 상태 (Java `VoiceSessionFacade.RuntimeContext`).
//!
//! Java의 두 잠금을 `Mutex` 두 개로 옮겼다.
//! - `audio` (Java `audioLock`): PCM 버퍼, 청크 순번, 현재 질문과 턴, 프리페치 캐시. Java가
//!   동시성 큐로 두던 대기 전사문과 연속 모드 턴도 여기에 둔다.
//! - `events` (Java `eventLock`): 싱크, 재전송 버퍼, 이벤트 id.
//!
//! 두 잠금 모두 짧게만 쥔다. 잠근 채로 유스케이스나 게이트웨이를 부르지 않는다. 싱크의 `send`는
//! Java처럼 `events`를 쥔 채로 부른다(이벤트 순서를 지키기 위해).
//! Java `synchronized`와 달리 `Mutex`는 다시 들어갈 수 없으므로, 싱크는 `send` 안에서 이 음성
//! 세션의 메서드(싱크 등록/해제 등)를 부르면 안 된다.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use omypic_domain::question::QuestionGroup;
use omypic_domain::shared::text_utils::{java_is_blank, java_trim};
use serde_json::{Map, Value};

use super::audio::DEFAULT_SAMPLE_RATE;
use super::settings::RuntimeSettings;
use crate::feedback::usecase::PrefetchedTurnPrompt;
use crate::question::model::NextQuestion;
use crate::voice::model::VoiceEventSink;
use crate::voice::usecase::{NowMillis, RecoveryQuestion, RecoverySnapshot};

const PREFETCH_CACHE_LIMIT: usize = 2;

/// 잠금이 오염돼도(다른 스레드가 잠근 채 패닉) 상태는 단순한 값이므로 계속 쓴다.
pub(super) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// 턴이 시작될 때의 현재 질문.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct QuestionSnapshot {
    pub(super) question_id: Option<String>,
    pub(super) text: Option<String>,
    pub(super) group_id: Option<String>,
    pub(super) group: Option<String>,
    pub(super) question_type: Option<String>,
}

pub(super) struct AudioSnapshot {
    pub(super) pcm16: Vec<u8>,
    pub(super) sample_rate: i32,
    pub(super) turn_id: i64,
    pub(super) question: Option<QuestionSnapshot>,
}

pub(super) struct TranscriptTurn {
    pub(super) text: String,
    pub(super) turn_id: i64,
    pub(super) question: Option<QuestionSnapshot>,
}

/// 피드백을 만들 질문과 답 하나.
#[derive(Debug, Clone)]
pub(super) struct TurnInput {
    pub(super) question_id: Option<String>,
    pub(super) question_text: Option<String>,
    pub(super) group_id: Option<String>,
    pub(super) question_group_text: Option<String>,
    pub(super) answer_text: String,
}

impl TurnInput {
    pub(super) fn question_group(&self) -> Option<QuestionGroup> {
        QuestionGroup::from_nullable(self.question_group_text.as_deref())
    }
}

struct VoiceEventRecord {
    event_id: i64,
    event: String,
    payload: Value,
}

struct AudioState {
    pcm_buffer: Vec<u8>,
    sample_rate: i32,
    last_accepted_chunk_sequence: Option<i64>,
    current_question: Option<NextQuestion>,
    current_turn_id: i64,
    /// 넣은 순서를 지킨다(Java `LinkedHashMap`).
    prefetch_by_question_id: Vec<PrefetchedTurnPrompt>,
    pending_transcripts: VecDeque<TranscriptTurn>,
    continuous_turns: VecDeque<TurnInput>,
}

struct EventState {
    sinks: Vec<Arc<dyn VoiceEventSink>>,
    replay_events: VecDeque<VoiceEventRecord>,
    last_event_id: i64,
}

pub(super) struct RuntimeContext {
    pub(super) voice_session_id: String,
    pub(super) session_id: String,
    pub(super) api_key: String,
    pub(super) feedback_api_key: String,
    pub(super) settings: RuntimeSettings,
    event_replay_buffer_limit: usize,
    audio: Mutex<AudioState>,
    events: Mutex<EventState>,
    initialized: AtomicBool,
    closed: AtomicBool,
    turn_processing: AtomicBool,
    turn_id_sequence: AtomicI64,
    tts_sequence: AtomicI64,
    stop_reason: Mutex<String>,
    stopped_at_epoch_ms: AtomicI64,
    now_millis: NowMillis,
    /// 마지막으로 무슨 일이 있던 시각: 클라이언트 요청, 싱크 등록/해제, 이벤트, 턴 끝.
    last_activity_epoch_ms: AtomicI64,
}

fn same_sink(a: &Arc<dyn VoiceEventSink>, b: &Arc<dyn VoiceEventSink>) -> bool {
    std::ptr::addr_eq(Arc::as_ptr(a), Arc::as_ptr(b))
}

impl RuntimeContext {
    pub(super) fn new(
        voice_session_id: String,
        session_id: String,
        api_key: String,
        feedback_api_key: String,
        settings: RuntimeSettings,
        event_replay_buffer_limit: usize,
        now_millis: NowMillis,
    ) -> Self {
        let opened_at = now_millis();
        Self {
            voice_session_id,
            session_id,
            api_key,
            feedback_api_key,
            settings,
            event_replay_buffer_limit: event_replay_buffer_limit.max(1),
            audio: Mutex::new(AudioState {
                pcm_buffer: Vec::new(),
                sample_rate: DEFAULT_SAMPLE_RATE,
                last_accepted_chunk_sequence: None,
                current_question: None,
                current_turn_id: 0,
                prefetch_by_question_id: Vec::new(),
                pending_transcripts: VecDeque::new(),
                continuous_turns: VecDeque::new(),
            }),
            events: Mutex::new(EventState {
                sinks: Vec::new(),
                replay_events: VecDeque::new(),
                last_event_id: 0,
            }),
            initialized: AtomicBool::new(false),
            closed: AtomicBool::new(false),
            turn_processing: AtomicBool::new(false),
            turn_id_sequence: AtomicI64::new(0),
            tts_sequence: AtomicI64::new(0),
            stop_reason: Mutex::new(String::new()),
            stopped_at_epoch_ms: AtomicI64::new(0),
            now_millis,
            last_activity_epoch_ms: AtomicI64::new(opened_at),
        }
    }

    /// 지금 무슨 일이 있었다고 적는다. 버려진 세션인지 볼 때 쓴다.
    pub(super) fn touch(&self) {
        self.last_activity_epoch_ms
            .store((self.now_millis)(), Ordering::SeqCst);
    }

    /// 진행 중인데 이벤트 스트림이 하나도 붙어 있지 않고, 처리 중인 턴도 없이 `idle_ms` 동안
    /// 아무 일도 없었으면 `true`. 클라이언트가 창을 닫았거나 죽어서 정지를 부르지 못한 세션이다.
    pub(super) fn is_abandoned(&self, now_epoch_ms: i64, idle_ms: i64) -> bool {
        if self.is_inactive() || self.turn_processing.load(Ordering::SeqCst) {
            return false;
        }
        {
            // 받는 쪽이 떠났는데 해제되지 않은 싱크(등록 중에 끊긴 SSE 요청 등)는 여기서 뺀다.
            let mut events = lock(&self.events);
            events.sinks.retain(|sink| !sink.is_closed());
            if !events.sinks.is_empty() {
                return false;
            }
        }
        now_epoch_ms - self.last_activity_epoch_ms.load(Ordering::SeqCst) >= idle_ms
    }

    /// 정지한 세션은 더 처리할 답이 없으므로 쌓인 오디오, 전사문, 연속 모드 턴, 프리페치를
    /// 놓는다. 복구 조회에 필요한 현재 질문, 순번, 재전송 버퍼는 남긴다.
    pub(super) fn release_pending_work(&self) {
        let mut audio = lock(&self.audio);
        audio.pcm_buffer = Vec::new();
        audio.pending_transcripts = VecDeque::new();
        audio.continuous_turns = VecDeque::new();
        audio.prefetch_by_question_id = Vec::new();
    }

    // ---- 싱크와 이벤트 ----

    /// `since_event_id`가 있으면 그 뒤의 이벤트를 먼저 다시 보낸다. 다시 보내다 실패하면 싱크를
    /// 등록하지 않는다. 같은 싱크는 한 번만 등록된다(Java `Set`).
    pub(super) fn add_sink(&self, sink: Arc<dyn VoiceEventSink>, since_event_id: Option<i64>) {
        self.touch();
        let mut events = lock(&self.events);
        if let Some(since) = since_event_id {
            if !replay_events(&events, sink.as_ref(), since) {
                return;
            }
        }
        if !events
            .sinks
            .iter()
            .any(|existing| same_sink(existing, &sink))
        {
            events.sinks.push(sink);
        }
    }

    pub(super) fn remove_sink(&self, sink: &Arc<dyn VoiceEventSink>) {
        self.touch();
        lock(&self.events)
            .sinks
            .retain(|existing| !same_sink(existing, sink));
    }

    /// payload에 `eventId`를 붙여 재전송 버퍼에 넣고 모든 싱크에 보낸다. `false`를 돌려준 싱크는
    /// 뺀다. 객체가 아닌 payload는 `{"eventId", "data"}`로 감싼다.
    pub(super) fn emit(&self, event: &str, payload: Value) {
        self.touch();
        let mut events = lock(&self.events);
        events.last_event_id += 1;
        let event_id = events.last_event_id;
        let record = VoiceEventRecord {
            event_id,
            event: event.to_string(),
            payload: append_event_id(payload, event_id),
        };
        let EventState {
            sinks,
            replay_events,
            ..
        } = &mut *events;
        sinks.retain(|sink| sink.send(&record.event, record.payload.clone()));
        replay_events.push_back(record);
        while replay_events.len() > self.event_replay_buffer_limit {
            replay_events.pop_front();
        }
    }

    // ---- 수명 ----

    pub(super) fn mark_initialized(&self) -> bool {
        self.initialized
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }

    /// 처음 닫을 때만 `true`.
    pub(super) fn close(&self) -> bool {
        self.closed
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }

    pub(super) fn is_inactive(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    pub(super) fn mark_stopped(&self, stop_reason: &str, stopped_at_epoch_ms: i64) {
        *lock(&self.stop_reason) = stop_reason.to_string();
        self.stopped_at_epoch_ms
            .store(stopped_at_epoch_ms, Ordering::SeqCst);
    }

    pub(super) fn is_stopped_expired(&self, now_epoch_ms: i64, retention_ms: i64) -> bool {
        if !self.is_inactive() || retention_ms < 0 {
            return false;
        }
        let stopped_at = self.stopped_at_epoch_ms.load(Ordering::SeqCst);
        if stopped_at <= 0 {
            return false;
        }
        now_epoch_ms - stopped_at >= retention_ms
    }

    /// 정렬용 정지 시각. 아직 기록되지 않았으면 가장 늦은 것으로 본다.
    pub(super) fn stopped_at_epoch_ms(&self) -> i64 {
        let stopped_at = self.stopped_at_epoch_ms.load(Ordering::SeqCst);
        if stopped_at <= 0 {
            i64::MAX
        } else {
            stopped_at
        }
    }

    pub(super) fn start_turn_processing(&self) -> bool {
        self.turn_processing
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }

    pub(super) fn finish_turn_processing(&self) {
        self.touch();
        self.turn_processing.store(false, Ordering::SeqCst);
    }

    pub(super) fn next_turn_id(&self) -> i64 {
        self.turn_id_sequence.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub(super) fn next_tts_sequence(&self) -> i64 {
        self.tts_sequence.fetch_add(1, Ordering::SeqCst) + 1
    }

    // ---- 오디오와 전사문 ----

    pub(super) fn append_audio(&self, pcm16: &[u8], sample_rate: i32) {
        let mut audio = lock(&self.audio);
        audio.pcm_buffer.extend_from_slice(pcm16);
        if sample_rate > 0 {
            audio.sample_rate = sample_rate;
        }
    }

    /// 순번이 없으면 받는다. 있으면 마지막으로 받은 순번보다 커야 받는다.
    pub(super) fn accept_chunk_sequence(&self, sequence: Option<i64>) -> bool {
        let Some(sequence) = sequence else {
            return true;
        };
        let mut audio = lock(&self.audio);
        if matches!(audio.last_accepted_chunk_sequence, Some(last) if sequence <= last) {
            return false;
        }
        audio.last_accepted_chunk_sequence = Some(sequence);
        true
    }

    pub(super) fn consume_audio_snapshot(&self) -> AudioSnapshot {
        let mut audio = lock(&self.audio);
        let pcm16 = std::mem::take(&mut audio.pcm_buffer);
        let sample_rate = if audio.sample_rate > 0 {
            audio.sample_rate
        } else {
            DEFAULT_SAMPLE_RATE
        };
        AudioSnapshot {
            pcm16,
            sample_rate,
            turn_id: audio.current_turn_id,
            question: snapshot_question(&audio),
        }
    }

    pub(super) fn has_buffered_audio(&self) -> bool {
        !lock(&self.audio).pcm_buffer.is_empty()
    }

    /// 전사문을 도착했을 때의 질문에 묶어 둔다. 턴을 처리할 때쯤에는 다음 질문이 이미 정해져
    /// 있을 수 있기 때문이다.
    pub(super) fn offer_transcript(&self, text: Option<&str>) {
        let mut audio = lock(&self.audio);
        let turn = TranscriptTurn {
            text: java_trim(text.unwrap_or("")).to_string(),
            turn_id: audio.current_turn_id,
            question: snapshot_question(&audio),
        };
        audio.pending_transcripts.push_back(turn);
    }

    pub(super) fn poll_transcript(&self) -> Option<TranscriptTurn> {
        lock(&self.audio).pending_transcripts.pop_front()
    }

    pub(super) fn has_pending_transcript(&self) -> bool {
        !lock(&self.audio).pending_transcripts.is_empty()
    }

    // ---- 현재 질문과 프리페치 ----

    pub(super) fn set_current_question(&self, question: NextQuestion, turn_id: i64) {
        let mut audio = lock(&self.audio);
        audio.current_question = Some(question);
        audio.current_turn_id = turn_id;
    }

    pub(super) fn clear_current_question(&self) {
        let mut audio = lock(&self.audio);
        audio.current_question = None;
        audio.prefetch_by_question_id.clear();
    }

    /// 지금 질문의 프리페치일 때만 넣는다. 캐시는 오래된 것부터 버려 2개까지 둔다.
    pub(super) fn store_prefetched_turn_prompt(&self, prefetch: PrefetchedTurnPrompt) {
        if java_is_blank(&prefetch.question_id) {
            return;
        }
        let mut audio = lock(&self.audio);
        let current_id = match &audio.current_question {
            Some(question) if !question.skipped => question.question_id.clone(),
            _ => return,
        };
        if current_id.as_deref() != Some(prefetch.question_id.as_str()) {
            return;
        }
        let cache = &mut audio.prefetch_by_question_id;
        match cache
            .iter_mut()
            .find(|cached| cached.question_id == prefetch.question_id)
        {
            Some(cached) => *cached = prefetch,
            None => cache.push(prefetch),
        }
        while cache.len() > PREFETCH_CACHE_LIMIT {
            cache.remove(0);
        }
    }

    pub(super) fn consume_prefetched_turn_prompt(
        &self,
        question_id: Option<&str>,
    ) -> Option<PrefetchedTurnPrompt> {
        let question_id = question_id.filter(|id| !java_is_blank(id))?;
        let mut audio = lock(&self.audio);
        let position = audio
            .prefetch_by_question_id
            .iter()
            .position(|cached| cached.question_id == question_id)?;
        Some(audio.prefetch_by_question_id.remove(position))
    }

    // ---- 연속 모드 ----

    pub(super) fn append_continuous_turn(&self, turn: TurnInput) {
        lock(&self.audio).continuous_turns.push_back(turn);
    }

    pub(super) fn poll_all_continuous_turns(&self) -> Vec<TurnInput> {
        lock(&self.audio).continuous_turns.drain(..).collect()
    }

    // ---- 복구 ----

    pub(super) fn snapshot_for_recovery(&self, last_seen_event_id: i64) -> RecoverySnapshot {
        let safe_last_seen = last_seen_event_id.max(0);
        let (question, turn_id, buffered_audio, accepted_chunk_sequence) = {
            let audio = lock(&self.audio);
            (
                snapshot_question(&audio),
                audio.current_turn_id,
                !audio.pcm_buffer.is_empty(),
                audio.last_accepted_chunk_sequence,
            )
        };
        let (oldest_event_id, latest_event_id) = {
            let events = lock(&self.events);
            let oldest = events
                .replay_events
                .front()
                .map_or(0, |record| record.event_id);
            (oldest, events.last_event_id)
        };
        let mut replay_from_event_id = safe_last_seen;
        let mut gap_detected = false;
        if oldest_event_id > 0 {
            let minimum_rewind_point = oldest_event_id - 1;
            replay_from_event_id = safe_last_seen.max(minimum_rewind_point);
            gap_detected = safe_last_seen < minimum_rewind_point;
        }
        let inactive = self.is_inactive();
        RecoverySnapshot {
            session_id: self.session_id.clone(),
            voice_session_id: self.voice_session_id.clone(),
            active: !inactive,
            stopped: inactive,
            stop_reason: lock(&self.stop_reason).clone(),
            current_turn_id: turn_id,
            current_question: question.map(|q| RecoveryQuestion {
                id: q.question_id,
                text: q.text,
                group: q.group,
                group_id: q.group_id,
                question_type: q.question_type,
            }),
            turn_processing: self.turn_processing.load(Ordering::SeqCst),
            has_buffered_audio: buffered_audio,
            last_accepted_chunk_sequence: accepted_chunk_sequence,
            latest_event_id,
            replay_from_event_id,
            gap_detected,
        }
    }
}

fn snapshot_question(audio: &AudioState) -> Option<QuestionSnapshot> {
    let question = audio.current_question.as_ref().filter(|q| !q.skipped)?;
    Some(QuestionSnapshot {
        question_id: question.question_id.clone(),
        text: question.text.clone(),
        group_id: question.group_id.clone(),
        group: question.group.clone(),
        question_type: question.question_type.clone(),
    })
}

fn replay_events(events: &EventState, sink: &dyn VoiceEventSink, since_event_id: i64) -> bool {
    let since = since_event_id.max(0);
    events
        .replay_events
        .iter()
        .filter(|record| record.event_id > since)
        .all(|record| sink.send(&record.event, record.payload.clone()))
}

fn append_event_id(payload: Value, event_id: i64) -> Value {
    match payload {
        Value::Object(mut map) => {
            map.insert("eventId".to_string(), Value::from(event_id));
            Value::Object(map)
        }
        other => {
            let mut wrapped = Map::new();
            wrapped.insert("eventId".to_string(), Value::from(event_id));
            wrapped.insert("data".to_string(), other);
            Value::Object(wrapped)
        }
    }
}
