//! 포트의 메모리 가짜 구현. 이후 작업의 테스트가 Mockito mock 대신 쓴다.
//!
//! 저장소 가짜는 저장된 값을 읽는 메서드를 열어 두고, 상호작용을 확인해야 하는 가짜는
//! 호출 기록을 돌려준다. 모든 가짜는 `&self`로 쓰며 내부에서 잠근다.

mod executor;
mod gateways;
mod metrics;
mod policies;
mod stores;

pub use executor::{DeferredExecutor, ImmediateExecutor, RecordingSink};
pub use gateways::{
    BootstrapCall, FakeApiKeyProbePort, FakeEmbeddingPort, FakeTtsGateway, GenerateCall, ProbeCall,
    ScriptedLlmClient, ScriptedSttGateway, SttCall, TtsCall,
};
pub use metrics::{MetricCall, RecordingMetrics};
pub use policies::{
    FakeDataDirProvider, FakeFeedbackPolicy, FakeRagPolicy, FakeSttPolicy, FakeVoicePolicy,
};
pub use stores::{
    InMemoryQuestionGroupPort, InMemoryRulebookFileStore, InMemoryRulebookIndexPort,
    InMemoryRulebookPort, InMemorySessionStorePort, InMemoryWrongNotePort,
    InMemoryWrongNoteRecentQueuePort, SearchCall,
};

use std::sync::{Mutex, MutexGuard};

/// 잠금이 오염돼도 테스트 가짜는 계속 쓴다.
pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stt::model::VadSettings;
    use crate::stt::port::SttGateway;
    use crate::voice::model::VoiceEventSink;
    use omypic_domain::error::AppError;

    #[test]
    fn scripted_stt_gateway_repeats_last_result_after_script_ends() {
        let vad = VadSettings {
            prefix_padding_ms: 1,
            silence_duration_ms: 2,
            threshold: 0.5,
        };
        let gateway = ScriptedSttGateway::new(vec![
            Err(AppError::Internal("first".to_string())),
            Ok("done".to_string()),
        ]);

        let results: Vec<_> = (0..3)
            .map(|_| gateway.transcribe(&[1], "m", "k", false, &vad))
            .collect();

        assert!(results[0].is_err());
        assert_eq!(Ok("done".to_string()), results[1]);
        assert_eq!(Ok("done".to_string()), results[2]);
        assert_eq!(3, gateway.calls().len());
    }

    #[test]
    fn deferred_executor_runs_tasks_on_demand_in_either_order() {
        use crate::shared::port::AsyncExecutor;
        use std::sync::Arc;

        let executor = Arc::new(DeferredExecutor::new());
        let log = Arc::new(std::sync::Mutex::new(Vec::new()));
        for name in ["a", "b", "c"] {
            let log = log.clone();
            executor.execute(Box::new(move || log.lock().unwrap().push(name)));
        }
        let nested_executor = executor.clone();
        let nested_log = log.clone();
        executor.execute(Box::new(move || {
            let log = nested_log.clone();
            nested_executor.execute(Box::new(move || log.lock().unwrap().push("nested")));
        }));

        assert_eq!(4, executor.pending());
        assert!(executor.run_next());
        assert!(executor.run_last());
        assert_eq!(3, executor.run_all());
        assert!(!executor.run_next());
        assert_eq!(vec!["a", "b", "c", "nested"], *log.lock().unwrap());
    }

    #[test]
    fn recording_sink_stops_recording_after_close() {
        let sink = RecordingSink::new();

        assert!(sink.send("a", serde_json::json!({"n": 1})));
        sink.close();
        assert!(!sink.send("b", serde_json::json!({})));

        assert_eq!(vec!["a"], sink.event_names());
        assert_eq!(vec![serde_json::json!({"n": 1})], sink.payloads("a"));
    }

    #[test]
    fn in_memory_file_store_round_trips_text() {
        use crate::rulebook::port::RulebookFileStore;
        let store = InMemoryRulebookFileStore::with_base_dir("/data/");

        let stored = store.store("a.md", "안녕".as_bytes()).unwrap();

        assert_eq!("/data/a.md", stored.path);
        assert_eq!("안녕", store.read_text(&stored.path).unwrap());
        assert!(store.read_bytes("/missing").is_err());
    }

    #[test]
    fn stores_record_delete_all_and_clear_in_call_order() {
        use crate::question::port::QuestionGroupPort;
        use crate::rulebook::port::RulebookPort;
        use crate::wrongnote::port::{WrongNotePort, WrongNoteRecentQueuePort};
        use omypic_domain::wrongnote::WrongNote;

        let groups = InMemoryQuestionGroupPort::new();
        let rulebooks = InMemoryRulebookPort::new();
        let notes = InMemoryWrongNotePort::new();
        let queue = InMemoryWrongNoteRecentQueuePort::new();

        groups.delete_all().unwrap();
        rulebooks.delete_all().unwrap();
        notes.delete_all().unwrap();
        notes
            .save(WrongNote::create_new("p", chrono::Utc::now()))
            .unwrap();
        queue.clear_global_queue().unwrap();
        queue.save_global_queue(&["p".to_string()]).unwrap();

        assert_eq!(1, groups.delete_all_calls());
        assert_eq!(1, rulebooks.delete_all_calls());
        assert_eq!(1, notes.delete_all_calls());
        assert_eq!(1, notes.save_calls());
        assert_eq!(vec!["delete_all", "save"], notes.call_log());
        assert_eq!(1, queue.clear_calls());
        assert_eq!(
            vec!["clear_global_queue", "save_global_queue"],
            queue.call_log()
        );
    }
}
