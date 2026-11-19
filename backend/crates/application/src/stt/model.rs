#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SttCommand {
    pub file_bytes: Option<Vec<u8>>,
    pub model: String,
    pub api_key: String,
    pub translate: bool,
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VadSettings {
    pub prefix_padding_ms: i32,
    pub silence_duration_ms: i32,
    pub threshold: f64,
}
