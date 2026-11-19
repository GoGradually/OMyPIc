/// 데이터 디렉터리 위치를 알려 준다.
pub trait DataDirProvider: Send + Sync {
    fn get_data_dir(&self) -> String;
}
