use std::sync::{Arc, Mutex, MutexGuard};

use omypic_domain::error::AppResult;
use omypic_domain::session::{SessionId, SessionState};

/// 세션 하나의 공유 가변 상태. 잠근 채로 오래 있지 않는다.
pub type SharedSessionState = Arc<Mutex<SessionState>>;

pub trait SessionStorePort: Send + Sync {
    /// 세션이 없으면 만들어서 돌려준다. 같은 id는 항상 같은 상태를 가리킨다.
    fn get_or_create(&self, session_id: &SessionId) -> AppResult<SharedSessionState>;
}

/// 세션 상태를 잠근다.
///
/// 잠금 방침(백엔드 전체): 다른 스레드가 잠근 채 패닉해 잠금이 오염돼도 안의 값을 그대로 쓴다.
/// 잠근 채로 하는 일은 단순한 값 갱신뿐이고, 오염을 오류로 돌리면 그 세션 id가 프로세스가
/// 끝날 때까지 쓸 수 없게 되기 때문이다.
pub fn lock_session(state: &SharedSessionState) -> MutexGuard<'_, SessionState> {
    state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poisoned_session_state_stays_usable() {
        let state: SharedSessionState =
            Arc::new(Mutex::new(SessionState::new(SessionId::of("s1").unwrap())));
        let poisoner = Arc::clone(&state);
        let _ = std::thread::spawn(move || {
            let _guard = poisoner.lock().unwrap();
            panic!("panic while holding the session lock");
        })
        .join();
        assert!(state.is_poisoned());

        assert_eq!("s1", lock_session(&state).session_id().value());
    }
}
