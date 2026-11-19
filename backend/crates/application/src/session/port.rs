use std::sync::{Arc, Mutex, MutexGuard};

use omypic_domain::error::{AppError, AppResult};
use omypic_domain::session::{SessionId, SessionState};

/// 세션 하나의 공유 가변 상태. 잠근 채로 오래 있지 않는다.
pub type SharedSessionState = Arc<Mutex<SessionState>>;

pub trait SessionStorePort: Send + Sync {
    /// 세션이 없으면 만들어서 돌려준다. 같은 id는 항상 같은 상태를 가리킨다.
    fn get_or_create(&self, session_id: &SessionId) -> AppResult<SharedSessionState>;
}

/// 세션 상태를 잠근다. 다른 스레드가 잠근 채 죽어 오염됐으면 내부 오류다.
pub fn lock_session(state: &SharedSessionState) -> AppResult<MutexGuard<'_, SessionState>> {
    state
        .lock()
        .map_err(|_| AppError::Internal("Session state lock is poisoned".to_string()))
}
