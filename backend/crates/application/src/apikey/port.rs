use omypic_domain::error::AppResult;

pub trait ApiKeyProbePort: Send + Sync {
    /// 키로 제공자에 실제 요청을 보내 본다. 실패하면 오류의 메시지가 검증 실패 메시지가 된다.
    fn probe(&self, provider: &str, api_key: &str, model: Option<&str>) -> AppResult<()>;
}
