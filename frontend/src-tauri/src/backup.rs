//! 백업 파일 쓰기/읽기. 대화상자는 `lib.rs`의 명령이 띄우고 고른 경로만 여기로 넘긴다.

use std::fs;
use std::path::Path;

use serde::Serialize;

pub const DEFAULT_BACKUP_NAME: &str = "omypic-backup.zip";
pub const FILTER_NAME: &str = "Zip Archive";
pub const FILTER_EXTENSIONS: &[&str] = &["zip"];

/// `pickBackupFile`이 돌려주는 파일.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PickedFile {
    pub name: String,
    pub bytes: Vec<u8>,
}

/// 저장 대화상자의 기본 파일 이름. 비었으면 `omypic-backup.zip`.
pub fn default_name(name: Option<String>) -> String {
    name.filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_BACKUP_NAME.to_string())
}

/// 고른 경로에 쓴다. 대화상자를 닫았으면(`None`) `false`.
pub fn write_backup(path: Option<&Path>, bytes: &[u8]) -> Result<bool, String> {
    let Some(path) = path else {
        return Ok(false);
    };
    fs::write(path, bytes).map_err(|err| format!("Failed to write {}: {err}", path.display()))?;
    Ok(true)
}

/// 고른 파일을 읽는다. 대화상자를 닫았으면(`None`) `None`.
pub fn read_backup(path: Option<&Path>) -> Result<Option<PickedFile>, String> {
    let Some(path) = path else {
        return Ok(None);
    };
    let bytes =
        fs::read(path).map_err(|err| format!("Failed to read {}: {err}", path.display()))?;
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok(Some(PickedFile { name, bytes }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_name_falls_back_to_backup_zip() {
        assert_eq!("omypic-backup.zip", default_name(None));
        assert_eq!("omypic-backup.zip", default_name(Some(" ".into())));
        assert_eq!("mine.zip", default_name(Some("mine.zip".into())));
    }

    #[test]
    fn cancelled_dialogs_do_nothing() {
        assert_eq!(Ok(false), write_backup(None, b"zip"));
        assert_eq!(Ok(None), read_backup(None));
    }

    #[test]
    fn written_backup_reads_back_with_file_name() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("backup.zip");

        assert_eq!(Ok(true), write_backup(Some(&path), &[1, 2, 3]));

        assert_eq!(
            Ok(Some(PickedFile {
                name: "backup.zip".into(),
                bytes: vec![1, 2, 3],
            })),
            read_backup(Some(&path))
        );
    }

    #[test]
    fn unreadable_backup_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read_backup(Some(&dir.path().join("missing.zip"))).is_err());
    }
}
