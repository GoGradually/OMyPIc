use std::sync::Arc;

use omypic_application::apikey::usecase::ApiKeyVerifyUseCase;
use omypic_application::datatransfer::usecase::DataTransferUseCase;
use omypic_application::question::usecase::QuestionUseCase;
use omypic_application::rulebook::usecase::RulebookUseCase;
use omypic_application::session::usecase::SessionUseCase;
use omypic_application::shared::port::MetricsReader;
use omypic_application::voice::policy::VoicePolicy;
use omypic_application::voice::usecase::VoiceSessionUseCase;
use omypic_application::wrongnote::usecase::WrongNoteUseCase;

/// 핸들러가 공유하는 유스케이스들. 복제는 `Arc` 복제뿐이다.
#[derive(Clone)]
pub struct AppState {
    pub api_key_verify: Arc<ApiKeyVerifyUseCase>,
    pub data_transfer: Arc<DataTransferUseCase>,
    pub question: Arc<QuestionUseCase>,
    pub rulebook: Arc<RulebookUseCase>,
    pub session: Arc<SessionUseCase>,
    pub voice_session: Arc<VoiceSessionUseCase>,
    pub wrong_note: Arc<WrongNoteUseCase>,
    /// `/api/meta/models`의 기본값을 정한다.
    pub voice_policy: Arc<dyn VoicePolicy>,
    /// `/actuator/metrics`가 읽는다.
    pub metrics: Arc<dyn MetricsReader>,
    /// 요청을 받아 주는 `Origin` 값(정확히 같은 문자열). 목록에 없는 `Origin`이 붙은 요청은 403이다.
    pub allowed_origins: Arc<Vec<String>>,
}
