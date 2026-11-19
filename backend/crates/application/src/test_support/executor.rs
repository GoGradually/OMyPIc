use std::collections::VecDeque;
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

type Task = Box<dyn FnOnce() + Send + 'static>;

/// 작업을 모아 두었다가 테스트가 원할 때 실행한다. 작업 안에서 새로 맡긴 작업은 큐 뒤에 붙는다.
/// 실행 중에는 큐를 잠그지 않으므로 작업이 다시 `execute`를 불러도 된다.
#[derive(Default)]
pub struct DeferredExecutor {
    tasks: Mutex<VecDeque<Task>>,
}

impl std::fmt::Debug for DeferredExecutor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeferredExecutor")
            .field("pending", &self.pending())
            .finish()
    }
}

impl DeferredExecutor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn pending(&self) -> usize {
        lock(&self.tasks).len()
    }

    /// 가장 먼저 맡긴 작업 하나를 실행한다. 작업이 없으면 `false`.
    pub fn run_next(&self) -> bool {
        let task = lock(&self.tasks).pop_front();
        task.map(|task| task()).is_some()
    }

    /// 가장 나중에 맡긴 작업 하나를 실행한다(먼저 맡긴 작업보다 늦게 끝나는 경우를 흉내 낸다).
    /// 작업이 없으면 `false`.
    pub fn run_last(&self) -> bool {
        let task = lock(&self.tasks).pop_back();
        task.map(|task| task()).is_some()
    }

    /// 큐가 빌 때까지 앞에서부터 실행한다. 실행한 작업 수를 돌려준다.
    pub fn run_all(&self) -> usize {
        let mut count = 0;
        while self.run_next() {
            count += 1;
        }
        count
    }
}

impl AsyncExecutor for DeferredExecutor {
    fn execute(&self, task: Task) {
        lock(&self.tasks).push_back(task);
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

    /// 지금까지 모은 이벤트를 버린다(Java 테스트의 `events.clear()`).
    pub fn clear(&self) {
        lock(&self.events).clear();
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
