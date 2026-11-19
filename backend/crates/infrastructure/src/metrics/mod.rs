use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::Duration;

use omypic_application::shared::port::{MetricSnapshot, MetricsPort, MetricsReader};

#[derive(Debug, Clone, Copy)]
enum Metric {
    Counter {
        count: u64,
    },
    Timer {
        count: u64,
        total: Duration,
        max: Duration,
    },
}

/// 메모리에만 쌓는 지표. 지표는 처음 기록될 때 등록된다(Micrometer와 같다).
#[derive(Default)]
pub struct InMemoryMetrics {
    metrics: Mutex<BTreeMap<&'static str, Metric>>,
}

impl InMemoryMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    fn with_metrics<R>(&self, f: impl FnOnce(&mut BTreeMap<&'static str, Metric>) -> R) -> R {
        // 지표는 단순 숫자라 다른 스레드가 패닉했어도 값을 그대로 쓴다.
        let mut guard = self
            .metrics
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        f(&mut guard)
    }

    fn record(&self, name: &'static str, duration: Duration) {
        self.with_metrics(|metrics| {
            let entry = metrics.entry(name).or_insert(Metric::Timer {
                count: 0,
                total: Duration::ZERO,
                max: Duration::ZERO,
            });
            if let Metric::Timer { count, total, max } = entry {
                *count += 1;
                *total += duration;
                *max = (*max).max(duration);
            }
        });
    }

    fn increment(&self, name: &'static str) {
        self.with_metrics(|metrics| {
            let entry = metrics.entry(name).or_insert(Metric::Counter { count: 0 });
            if let Metric::Counter { count } = entry {
                *count += 1;
            }
        });
    }
}

impl MetricsPort for InMemoryMetrics {
    fn record_stt_latency(&self, duration: Duration) {
        self.record("stt.latency", duration);
    }

    fn record_feedback_latency(&self, duration: Duration) {
        self.record("feedback.latency", duration);
    }

    fn record_tts_latency(&self, duration: Duration) {
        self.record("tts.latency", duration);
    }

    fn record_question_next_latency(&self, duration: Duration) {
        self.record("question.next.latency", duration);
    }

    fn record_voice_turn_latency(&self, duration: Duration) {
        self.record("voice.turn.latency", duration);
    }

    fn record_rulebook_upload_latency(&self, duration: Duration) {
        self.record("rulebook.upload.latency", duration);
    }

    fn increment_stt_request(&self) {
        self.increment("stt.requests");
    }

    fn increment_stt_error(&self) {
        self.increment("stt.errors");
    }

    fn increment_feedback_error(&self) {
        self.increment("feedback.errors");
    }

    fn increment_feedback_schema_fallback(&self) {
        self.increment("feedback.schema_fallbacks");
    }

    fn increment_tts_error(&self) {
        self.increment("tts.errors");
    }

    fn increment_recommendation_duplicate_detected(&self) {
        self.increment("feedback.recommendation.duplicate_detected");
    }

    fn increment_recommendation_repair_attempt(&self) {
        self.increment("feedback.recommendation.repair_attempt");
    }

    fn increment_recommendation_repair_success(&self) {
        self.increment("feedback.recommendation.repair_success");
    }

    fn increment_recommendation_minimal_fallback(&self) {
        self.increment("feedback.recommendation.minimal_fallback");
    }
}

impl MetricsReader for InMemoryMetrics {
    fn names(&self) -> Vec<String> {
        self.with_metrics(|metrics| metrics.keys().map(|name| name.to_string()).collect())
    }

    fn snapshot(&self, name: &str) -> Option<MetricSnapshot> {
        self.with_metrics(|metrics| {
            let measurements = match metrics.get(name)? {
                Metric::Counter { count } => vec![("COUNT".to_string(), *count as f64)],
                Metric::Timer { count, total, max } => vec![
                    ("COUNT".to_string(), *count as f64),
                    ("TOTAL_TIME".to_string(), total.as_secs_f64()),
                    ("MAX".to_string(), max.as_secs_f64()),
                ],
            };
            Some(MetricSnapshot {
                name: name.to_string(),
                measurements,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value(metrics: &InMemoryMetrics, name: &str, stat: &str) -> f64 {
        metrics
            .snapshot(name)
            .unwrap_or_else(|| panic!("missing metric {name}"))
            .measurements
            .into_iter()
            .find(|(key, _)| key == stat)
            .unwrap_or_else(|| panic!("missing stat {stat}"))
            .1
    }

    #[test]
    fn records_timers_and_error_counters() {
        let metrics = InMemoryMetrics::new();

        metrics.record_stt_latency(Duration::from_millis(10));
        metrics.record_feedback_latency(Duration::from_millis(20));
        metrics.record_tts_latency(Duration::from_millis(30));
        metrics.record_question_next_latency(Duration::from_millis(40));
        metrics.record_voice_turn_latency(Duration::from_millis(50));
        metrics.record_rulebook_upload_latency(Duration::from_millis(60));

        metrics.increment_stt_request();
        metrics.increment_stt_error();
        metrics.increment_feedback_error();
        metrics.increment_feedback_schema_fallback();
        metrics.increment_tts_error();
        metrics.increment_recommendation_duplicate_detected();
        metrics.increment_recommendation_repair_attempt();
        metrics.increment_recommendation_repair_success();
        metrics.increment_recommendation_minimal_fallback();

        for timer in [
            "stt.latency",
            "feedback.latency",
            "tts.latency",
            "question.next.latency",
            "voice.turn.latency",
            "rulebook.upload.latency",
        ] {
            assert_eq!(1.0, value(&metrics, timer, "COUNT"), "{timer}");
        }
        for counter in [
            "stt.requests",
            "stt.errors",
            "feedback.errors",
            "feedback.schema_fallbacks",
            "tts.errors",
            "feedback.recommendation.duplicate_detected",
            "feedback.recommendation.repair_attempt",
            "feedback.recommendation.repair_success",
            "feedback.recommendation.minimal_fallback",
        ] {
            assert_eq!(1.0, value(&metrics, counter, "COUNT"), "{counter}");
        }
        assert_eq!(15, metrics.names().len());
    }

    #[test]
    fn timer_reports_count_total_time_and_max_in_seconds() {
        let metrics = InMemoryMetrics::new();

        metrics.record_stt_latency(Duration::from_millis(100));
        metrics.record_stt_latency(Duration::from_millis(300));

        let snapshot = metrics.snapshot("stt.latency").unwrap();
        assert_eq!("stt.latency", snapshot.name);
        let stats: Vec<&str> = snapshot
            .measurements
            .iter()
            .map(|(k, _)| k.as_str())
            .collect();
        assert_eq!(vec!["COUNT", "TOTAL_TIME", "MAX"], stats);
        assert_eq!(2.0, value(&metrics, "stt.latency", "COUNT"));
        assert!((value(&metrics, "stt.latency", "TOTAL_TIME") - 0.4).abs() < 1e-9);
        assert!((value(&metrics, "stt.latency", "MAX") - 0.3).abs() < 1e-9);
    }

    #[test]
    fn counter_reports_only_count_and_unknown_name_is_none() {
        let metrics = InMemoryMetrics::new();

        metrics.increment_stt_request();
        metrics.increment_stt_request();

        let snapshot = metrics.snapshot("stt.requests").unwrap();
        assert_eq!(vec![("COUNT".to_string(), 2.0)], snapshot.measurements);
        assert!(metrics.snapshot("stt.errors").is_none());
        assert_eq!(vec!["stt.requests".to_string()], metrics.names());
    }
}
