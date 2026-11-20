use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use omypic_application::session::port::{SessionStorePort, SharedSessionState};
use omypic_domain::error::AppResult;
use omypic_domain::session::{SessionId, SessionState};

/// 기본으로 보관하는 세션 상태 수. 데스크톱 앱은 학습자 한 명이 세션 id 하나를 계속 쓴다.
pub const DEFAULT_SESSION_STATE_MAX: usize = 64;

struct Entry {
    state: SharedSessionState,
    last_used: u64,
}

struct Sessions {
    entries: HashMap<SessionId, Entry>,
    clock: u64,
}

/// 세션 상태를 메모리에만 보관한다. 세션 id는 클라이언트가 정하므로 상한(`capacity`)을 넘으면
/// 가장 오래 쓰지 않은 상태부터 버린다. 버린 세션 id가 다시 오면 새 상태로 시작한다.
pub struct InMemorySessionStore {
    sessions: Mutex<Sessions>,
    capacity: usize,
}

impl Default for InMemorySessionStore {
    fn default() -> Self {
        Self::with_capacity(DEFAULT_SESSION_STATE_MAX)
    }
}

impl InMemorySessionStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// `capacity`는 1보다 작으면 1이다.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            sessions: Mutex::new(Sessions {
                entries: HashMap::new(),
                clock: 0,
            }),
            capacity: capacity.max(1),
        }
    }

    /// 보관 중인 세션 상태 수.
    pub fn len(&self) -> usize {
        self.lock().entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 상태는 단순한 값이고 잠근 채 패닉할 만한 일을 하지 않으므로 오염돼도 계속 쓴다
    /// (`omypic_application::session::port::lock_session`과 같은 방침).
    fn lock(&self) -> std::sync::MutexGuard<'_, Sessions> {
        self.sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl SessionStorePort for InMemorySessionStore {
    fn get_or_create(&self, session_id: &SessionId) -> AppResult<SharedSessionState> {
        let mut sessions = self.lock();
        sessions.clock += 1;
        let now = sessions.clock;
        if let Some(entry) = sessions.entries.get_mut(session_id) {
            entry.last_used = now;
            return Ok(Arc::clone(&entry.state));
        }
        if sessions.entries.len() >= self.capacity {
            let oldest = sessions
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(id, _)| id.clone());
            if let Some(oldest) = oldest {
                sessions.entries.remove(&oldest);
            }
        }
        let state = Arc::new(Mutex::new(SessionState::new(session_id.clone())));
        sessions.entries.insert(
            session_id.clone(),
            Entry {
                state: Arc::clone(&state),
                last_used: now,
            },
        );
        Ok(state)
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

    #[test]
    fn store_keeps_at_most_its_capacity_dropping_the_least_recently_used() {
        let store = InMemorySessionStore::with_capacity(2);
        let id = |value: &str| SessionId::of(value).unwrap();

        let first = store.get_or_create(&id("s1")).unwrap();
        store.get_or_create(&id("s2")).unwrap();
        // s1 is used again, so s2 is now the least recently used.
        assert!(Arc::ptr_eq(
            &first,
            &store.get_or_create(&id("s1")).unwrap()
        ));
        store.get_or_create(&id("s3")).unwrap();

        assert_eq!(2, store.len());
        assert!(Arc::ptr_eq(
            &first,
            &store.get_or_create(&id("s1")).unwrap()
        ));
        assert_eq!(2, store.len());
    }

    #[test]
    fn default_capacity_bounds_many_distinct_sessions() {
        let store = InMemorySessionStore::new();
        for n in 0..(DEFAULT_SESSION_STATE_MAX * 3) {
            store
                .get_or_create(&SessionId::of(&format!("s{n}")).unwrap())
                .unwrap();
        }
        assert_eq!(DEFAULT_SESSION_STATE_MAX, store.len());
    }
}
