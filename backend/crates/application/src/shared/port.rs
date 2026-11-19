use std::time::Duration;

/// 백그라운드에서 작업을 실행한다.
pub trait AsyncExecutor: Send + Sync {
    fn execute(&self, task: Box<dyn FnOnce() + Send + 'static>);
}

/// 지연 시간과 횟수 지표를 기록한다.
pub trait MetricsPort: Send + Sync {
    fn record_stt_latency(&self, duration: Duration);
    fn record_feedback_latency(&self, duration: Duration);
    fn record_tts_latency(&self, duration: Duration);
    fn record_question_next_latency(&self, duration: Duration);
    fn record_voice_turn_latency(&self, duration: Duration);
    fn record_rulebook_upload_latency(&self, duration: Duration);
    fn increment_stt_request(&self);
    fn increment_stt_error(&self);
    fn increment_feedback_error(&self);
    fn increment_feedback_schema_fallback(&self);
    fn increment_tts_error(&self);
    fn increment_recommendation_duplicate_detected(&self);
    fn increment_recommendation_repair_attempt(&self);
    fn increment_recommendation_repair_success(&self);
    fn increment_recommendation_minimal_fallback(&self);
}

/// 지표 하나의 현재 값. `measurements`는 통계 이름(`COUNT`, `TOTAL_TIME`, `MAX` 등)과 값이다.
#[derive(Debug, Clone, PartialEq)]
pub struct MetricSnapshot {
    pub name: String,
    pub measurements: Vec<(String, f64)>,
}

/// 기록된 지표를 읽는 쪽 인터페이스(`/actuator/metrics`용).
pub trait MetricsReader: Send + Sync {
    fn names(&self) -> Vec<String>;
    fn snapshot(&self, name: &str) -> Option<MetricSnapshot>;
}
