use std::path::PathBuf;

use omypic_application::shared::policy::DataDirProvider;

/// 기본 데이터 디렉터리: `<홈>/OMyPIc`. 홈을 알 수 없으면 현재 디렉터리 아래에 둔다.
pub fn default_data_dir() -> String {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join("OMyPIc").to_string_lossy().into_owned()
}

/// 고정된 경로를 돌려주는 `DataDirProvider`.
#[derive(Debug, Clone)]
pub struct FixedDataDir(pub String);

impl DataDirProvider for FixedDataDir {
    fn get_data_dir(&self) -> String {
        self.0.clone()
    }
}
