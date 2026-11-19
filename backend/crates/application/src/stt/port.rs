use omypic_domain::error::AppResult;

use super::model::VadSettings;

pub trait SttGateway: Send + Sync {
    fn transcribe(
        &self,
        file_bytes: &[u8],
        model: &str,
        api_key: &str,
        translate: bool,
        vad_settings: &VadSettings,
    ) -> AppResult<String>;
}
