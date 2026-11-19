use std::sync::Mutex;
use std::time::Duration;

use super::lock;
use crate::shared::port::MetricsPort;

/// 지표 호출 하나. `name`은 `MetricsPort` 메서드 이름(`record_stt_latency` 등)이고,
/// 지연 시간을 기록한 호출이면 `duration`이 있다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetricCall {
    pub name: &'static str,
    pub duration: Option<Duration>,
}

/// 호출을 순서대로 기록한다.
#[derive(Debug, Default)]
pub struct RecordingMetrics {
    calls: Mutex<Vec<MetricCall>>,
}

impl RecordingMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn calls(&self) -> Vec<MetricCall> {
        lock(&self.calls).clone()
    }

    /// `name` 메서드가 불린 횟수.
    pub fn count(&self, name: &str) -> usize {
        lock(&self.calls).iter().filter(|c| c.name == name).count()
    }

    fn record(&self, name: &'static str, duration: Option<Duration>) {
        lock(&self.calls).push(MetricCall { name, duration });
    }
}

macro_rules! latency {
    ($($method:ident),*) => {$(
        fn $method(&self, duration: Duration) {
            self.record(stringify!($method), Some(duration));
        }
    )*};
}

macro_rules! counter {
    ($($method:ident),*) => {$(
        fn $method(&self) {
            self.record(stringify!($method), None);
        }
    )*};
}

impl MetricsPort for RecordingMetrics {
    latency!(
        record_stt_latency,
        record_feedback_latency,
        record_tts_latency,
        record_question_next_latency,
        record_voice_turn_latency,
        record_rulebook_upload_latency
    );
    counter!(
        increment_stt_request,
        increment_stt_error,
        increment_feedback_error,
        increment_feedback_schema_fallback,
        increment_tts_error,
        increment_recommendation_duplicate_detected,
        increment_recommendation_repair_attempt,
        increment_recommendation_repair_success,
        increment_recommendation_minimal_fallback
    );
}
