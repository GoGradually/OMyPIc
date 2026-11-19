use super::model::VadSettings;

pub trait SttPolicy: Send + Sync {
    fn get_max_file_bytes(&self) -> u64;
    fn retry_max(&self) -> u32;
    fn get_vad_settings(&self) -> VadSettings;
}
