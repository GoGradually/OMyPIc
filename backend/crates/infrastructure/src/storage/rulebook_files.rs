use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use omypic_application::rulebook::model::StoredRulebookFile;
use omypic_application::rulebook::port::RulebookFileStore;
use omypic_application::shared::policy::DataDirProvider;
use omypic_domain::error::{AppError, AppResult};

/// 룰북 원본 파일을 `<data-dir>/rulebooks/` 아래에 저장한다.
pub struct FileSystemRulebookStore {
    data_dir_provider: Arc<dyn DataDirProvider>,
}

impl FileSystemRulebookStore {
    pub fn new(data_dir_provider: Arc<dyn DataDirProvider>) -> Self {
        Self { data_dir_provider }
    }

    fn rulebook_dir(&self) -> PathBuf {
        Path::new(&self.data_dir_provider.get_data_dir()).join("rulebooks")
    }
}

/// 파일 이름에서 디렉터리 구분자(`/`, `\`)가 붙은 앞부분을 버린다.
/// Java 구현은 이름을 그대로 `resolve`해서 `../`가 디렉터리를 벗어났다. 여기서는 마지막 구성 요소만 쓴다.
fn sanitize_filename(filename: &str) -> String {
    let last = filename
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .replace('\0', "");
    match last.as_str() {
        "" | "." | ".." => "rulebook".to_string(),
        _ => last,
    }
}

fn io_error(context: &str, path: &Path, error: std::io::Error) -> AppError {
    AppError::Internal(format!("{context}: {} ({error})", path.display()))
}

impl RulebookFileStore for FileSystemRulebookStore {
    fn store(&self, filename: &str, bytes: &[u8]) -> AppResult<StoredRulebookFile> {
        let dir = self.rulebook_dir();
        fs::create_dir_all(&dir).map_err(|e| io_error("Failed to create rulebook dir", &dir, e))?;
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let target = dir.join(format!("{millis}_{}", sanitize_filename(filename)));
        fs::write(&target, bytes).map_err(|e| io_error("Failed to write rulebook", &target, e))?;
        Ok(StoredRulebookFile {
            path: target.to_string_lossy().into_owned(),
        })
    }

    fn read_text(&self, path: &str) -> AppResult<String> {
        fs::read_to_string(path)
            .map_err(|e| io_error("Failed to read rulebook", Path::new(path), e))
    }

    fn read_bytes(&self, path: &str) -> AppResult<Vec<u8>> {
        fs::read(path).map_err(|e| io_error("Failed to read rulebook", Path::new(path), e))
    }

    fn clear_all(&self) -> AppResult<()> {
        let dir = self.rulebook_dir();
        if !dir.exists() {
            return Ok(());
        }
        let entries =
            fs::read_dir(&dir).map_err(|e| io_error("Failed to list rulebook dir", &dir, e))?;
        for entry in entries {
            let entry = entry.map_err(|e| io_error("Failed to list rulebook dir", &dir, e))?;
            let path = entry.path();
            let is_dir = entry
                .file_type()
                .map_err(|e| io_error("Failed to clear rulebook path", &path, e))?
                .is_dir();
            let result = if is_dir {
                fs::remove_dir_all(&path)
            } else {
                fs::remove_file(&path)
            };
            result.map_err(|e| {
                AppError::InvalidState(format!(
                    "Failed to clear rulebook path: {} ({e})",
                    path.display()
                ))
            })?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::FixedDataDir;

    fn store_in(dir: &Path) -> FileSystemRulebookStore {
        FileSystemRulebookStore::new(Arc::new(FixedDataDir(dir.to_string_lossy().into_owned())))
    }

    #[test]
    fn store_writes_file_under_rulebook_directory() {
        let temp = tempfile::tempdir().unwrap();
        let store = store_in(temp.path());

        let stored = store.store("rules.md", b"hello markdown").unwrap();

        let path = PathBuf::from(&stored.path);
        assert!(path.starts_with(temp.path().join("rulebooks")));
        assert!(path.exists());
        assert_eq!("hello markdown", fs::read_to_string(&path).unwrap());
    }

    #[test]
    fn read_text_reads_utf8_text() {
        let temp = tempfile::tempdir().unwrap();
        let store = store_in(temp.path());
        let file = temp.path().join("manual.md");
        fs::write(&file, "plain ascii text").unwrap();

        let text = store.read_text(&file.to_string_lossy()).unwrap();

        assert_eq!("plain ascii text", text);
    }

    #[test]
    fn rulebook_store_keeps_korean_filename_bytes() {
        let temp = tempfile::tempdir().unwrap();
        let store = store_in(temp.path());

        let stored = store.store("오픽 규칙.md", "한글 내용".as_bytes()).unwrap();

        assert!(stored.path.ends_with("오픽 규칙.md"));
        assert_eq!("한글 내용", store.read_text(&stored.path).unwrap());
        assert_eq!(
            "한글 내용".as_bytes(),
            store.read_bytes(&stored.path).unwrap().as_slice()
        );
    }

    fn assert_stays_inside(temp: &Path, filename: &str, expected_suffix: &str) {
        let base = temp.join("data");
        let store = store_in(&base);

        let stored = store.store(filename, b"x").unwrap();

        let path = PathBuf::from(&stored.path);
        assert_eq!(base.join("rulebooks"), path.parent().unwrap());
        assert!(
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .ends_with(&format!("_{expected_suffix}")),
            "unexpected stored name: {}",
            stored.path
        );
        // 데이터 디렉터리 밖에는 아무것도 생기지 않는다.
        let outside: Vec<_> = fs::read_dir(temp)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(vec!["data".to_string()], outside);
    }

    #[test]
    fn store_does_not_escape_with_parent_directory_name() {
        let temp = tempfile::tempdir().unwrap();
        assert_stays_inside(temp.path(), "../../evil.md", "evil.md");
    }

    #[test]
    fn store_does_not_escape_with_nested_path_name() {
        let temp = tempfile::tempdir().unwrap();
        assert_stays_inside(temp.path(), "a/b.md", "b.md");
    }

    #[test]
    fn store_does_not_escape_with_windows_style_name() {
        let temp = tempfile::tempdir().unwrap();
        assert_stays_inside(temp.path(), "C:\\x.md", "x.md");
    }

    #[test]
    fn store_replaces_dot_only_names() {
        let temp = tempfile::tempdir().unwrap();
        assert_stays_inside(temp.path(), "..", "rulebook");
        let temp = tempfile::tempdir().unwrap();
        assert_stays_inside(temp.path(), "", "rulebook");
    }

    #[test]
    fn clear_all_removes_contents_but_keeps_directory() {
        let temp = tempfile::tempdir().unwrap();
        let store = store_in(temp.path());
        store.store("a.md", b"a").unwrap();
        let nested = temp.path().join("rulebooks").join("nested");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("b.md"), "b").unwrap();

        store.clear_all().unwrap();

        let dir = temp.path().join("rulebooks");
        assert!(dir.exists());
        assert_eq!(0, fs::read_dir(&dir).unwrap().count());
    }

    #[test]
    fn clear_all_without_directory_is_noop() {
        let temp = tempfile::tempdir().unwrap();
        store_in(temp.path()).clear_all().unwrap();
    }
}
