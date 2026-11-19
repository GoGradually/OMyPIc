use omypic_domain::error::AppResult;

pub trait TtsGateway: Send + Sync {
    fn synthesize(&self, api_key: &str, model: &str, voice: &str, text: &str)
        -> AppResult<Vec<u8>>;
}
