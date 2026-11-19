use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use omypic_application::session::port::{SessionStorePort, SharedSessionState};
use omypic_domain::error::{AppError, AppResult};
use omypic_domain::session::{SessionId, SessionState};

/// 세션 상태를 메모리에만 보관한다.
#[derive(Default)]
pub struct InMemorySessionStore {
    sessions: Mutex<HashMap<SessionId, SharedSessionState>>,
}

impl InMemorySessionStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl SessionStorePort for InMemorySessionStore {
    fn get_or_create(&self, session_id: &SessionId) -> AppResult<SharedSessionState> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| AppError::Internal("Session store lock is poisoned".to_string()))?;
        let state = sessions
            .entry(session_id.clone())
            .or_insert_with(|| Arc::new(Mutex::new(SessionState::new(session_id.clone()))));
        Ok(Arc::clone(state))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_or_create_returns_same_state_for_same_session_id() {
        let store = InMemorySessionStore::new();

        let first = store.get_or_create(&SessionId::of("s1").unwrap()).unwrap();
        first.lock().unwrap().append_segment("hello");
        let second = store.get_or_create(&SessionId::of("s1").unwrap()).unwrap();

        assert!(Arc::ptr_eq(&first, &second));
        let segments = second.lock().unwrap().stt_segments();
        assert_eq!(1, segments.len());
        assert_eq!("hello", segments[0]);
    }

    #[test]
    fn get_or_create_separates_different_session_ids() {
        let store = InMemorySessionStore::new();

        let first = store.get_or_create(&SessionId::of("s1").unwrap()).unwrap();
        let other = store.get_or_create(&SessionId::of("s2").unwrap()).unwrap();

        assert!(!Arc::ptr_eq(&first, &other));
    }
}
