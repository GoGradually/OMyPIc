use std::sync::Mutex;

use serde_json::Value;

use super::lock;
use crate::shared::port::AsyncExecutor;
use crate::voice::model::VoiceEventSink;

/// 작업을 호출한 스레드에서 바로 실행한다.
#[derive(Debug, Default)]
pub struct ImmediateExecutor;

impl AsyncExecutor for ImmediateExecutor {
    fn execute(&self, task: Box<dyn FnOnce() + Send + 'static>) {
        task();
    }
}

/// 보낸 이벤트를 모은다. `close()` 뒤에는 `send`가 `false`를 돌려주고 이벤트를 모으지 않는다.
#[derive(Debug, Default)]
pub struct RecordingSink {
    events: Mutex<Vec<(String, Value)>>,
    closed: Mutex<bool>,
}

impl RecordingSink {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn close(&self) {
        *lock(&self.closed) = true;
    }

    pub fn events(&self) -> Vec<(String, Value)> {
        lock(&self.events).clone()
    }

    pub fn event_names(&self) -> Vec<String> {
        lock(&self.events)
            .iter()
            .map(|(name, _)| name.clone())
            .collect()
    }

    /// 이름이 `event`인 이벤트의 payload만, 보낸 순서대로.
    pub fn payloads(&self, event: &str) -> Vec<Value> {
        lock(&self.events)
            .iter()
            .filter(|(name, _)| name == event)
            .map(|(_, payload)| payload.clone())
            .collect()
    }
}

impl VoiceEventSink for RecordingSink {
    fn send(&self, event: &str, payload: Value) -> bool {
        if *lock(&self.closed) {
            return false;
        }
        lock(&self.events).push((event.to_string(), payload));
        true
    }
}
