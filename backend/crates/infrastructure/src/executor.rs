use std::any::Any;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::thread;

use omypic_application::shared::port::AsyncExecutor;

/// 작업마다 새 스레드를 띄우는 실행기. 작업이 패닉해도 로그만 남기고 다른 작업에는 영향이 없다.
#[derive(Debug, Default, Clone, Copy)]
pub struct ThreadPoolExecutor;

impl ThreadPoolExecutor {
    pub fn new() -> Self {
        Self
    }
}

impl AsyncExecutor for ThreadPoolExecutor {
    fn execute(&self, task: Box<dyn FnOnce() + Send + 'static>) {
        thread::spawn(move || {
            if let Err(payload) = catch_unwind(AssertUnwindSafe(task)) {
                tracing::error!("Async task panicked: {}", panic_message(payload.as_ref()));
            }
        });
    }
}

fn panic_message(payload: &(dyn Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "unknown panic payload".to_string()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::Duration;

    use super::*;

    #[test]
    fn execute_runs_task_on_another_thread() {
        let executor = ThreadPoolExecutor::new();
        let (sender, receiver) = mpsc::channel();
        let caller = thread::current().id();

        executor.execute(Box::new(move || {
            sender.send(thread::current().id()).unwrap();
        }));

        let worker = receiver.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_ne!(caller, worker);
    }

    #[test]
    fn panicking_task_does_not_prevent_later_task() {
        let executor = ThreadPoolExecutor::new();
        let (sender, receiver) = mpsc::channel();

        executor.execute(Box::new(|| panic!("boom")));
        executor.execute(Box::new(move || {
            sender.send("done").unwrap();
        }));

        assert_eq!(
            "done",
            receiver.recv_timeout(Duration::from_secs(5)).unwrap()
        );
    }
}
