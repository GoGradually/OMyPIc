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
/// 지금 아무도 쥐고 있지 않은 상태 가운데 가장 오래 쓰지 않은 것부터 버린다. 버린 세션 id가
/// 다시 오면 새 상태로 시작한다.
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
        while sessions.entries.len() >= self.capacity {
            // 다른 곳에서 아직 쥔 상태(진행 중인 턴 등)는 버리지 않는다. 버리면 그쪽이 고아가 된
            // 상태에 쓰고, 같은 id로 새 상태가 따로 생긴다. 모두 쓰는 중이면 잠시 상한을 넘긴다.
            let oldest_unused = sessions
                .entries
                .iter()
                .filter(|(_, entry)| Arc::strong_count(&entry.state) == 1)
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(id, _)| id.clone());
            let Some(oldest) = oldest_unused else {
                break;
            };
            sessions.entries.remove(&oldest);
            tracing::debug!(
                session_id = oldest.value(),
                "Evicted the least recently used session state"
            );
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
    fn a_state_still_in_use_is_never_evicted() {
        let store = InMemorySessionStore::with_capacity(2);
        let id = |value: &str| SessionId::of(value).unwrap();

        // s1 is the least recently used but a running turn still holds it.
        let held = store.get_or_create(&id("s1")).unwrap();
        store.get_or_create(&id("s2")).unwrap();
        store.get_or_create(&id("s3")).unwrap();

        assert!(Arc::ptr_eq(&held, &store.get_or_create(&id("s1")).unwrap()));
        assert_eq!(2, store.len());
    }

    #[test]
    fn the_store_grows_past_its_capacity_while_every_state_is_in_use() {
        let store = InMemorySessionStore::with_capacity(2);
        let id = |value: &str| SessionId::of(value).unwrap();

        let held: Vec<_> = ["s1", "s2", "s3"]
            .iter()
            .map(|value| store.get_or_create(&id(value)).unwrap())
            .collect();

        assert_eq!(3, store.len());
        assert!(Arc::ptr_eq(
            &held[0],
            &store.get_or_create(&id("s1")).unwrap()
        ));
        drop(held);
        store.get_or_create(&id("s4")).unwrap();
        assert_eq!(
            2,
            store.len(),
            "unused states are evicted again once released"
        );
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
