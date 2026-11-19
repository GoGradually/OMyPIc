use omypic_domain::session::ModeType;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModeUpdateCommand {
    /// 비어 있으면 `SessionId`가 "SessionId is required"로 거절한다.
    pub session_id: String,
    pub mode: Option<ModeType>,
    pub continuous_batch_size: Option<i32>,
    pub selected_group_tags: Vec<String>,
}
