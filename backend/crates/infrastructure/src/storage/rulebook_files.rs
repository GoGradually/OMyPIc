use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use omypic_application::rulebook::model::StoredRulebookFile;
use omypic_application::rulebook::port::RulebookFileStore;
use omypic_application::shared::policy::DataDirProvider;
use omypic_domain::error::{AppError, AppResult};
use uuid::Uuid;

/// 저장 이름이 우연히 겹칠 때 다시 시도하는 횟수.
const STORE_ATTEMPTS: usize = 8;

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
        let name = sanitize_filename(filename);
        // 시각 대신 무작위 접두어를 써서 같은 이름이 연달아 와도 경로가 겹치지 않는다.
        // `create_new`라서 혹시 겹치면 덮어쓰지 않고 다른 이름으로 다시 만든다.
        for _ in 0..STORE_ATTEMPTS {
            let target = dir.join(format!("{}_{name}", Uuid::new_v4().simple()));
            let mut file = match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&target)
            {
                Ok(file) => file,
                Err(e) if e.kind() == ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(io_error("Failed to write rulebook", &target, e)),
            };
            if let Err(e) = file.write_all(bytes).and_then(|()| file.sync_all()) {
                drop(file);
                let _ = fs::remove_file(&target);
                return Err(io_error("Failed to write rulebook", &target, e));
            }
            return Ok(StoredRulebookFile {
                path: target.to_string_lossy().into_owned(),
            });
        }
        Err(AppError::Internal(format!(
            "Failed to find a free rulebook file name in {}",
            dir.display()
        )))
    }

    fn read_text(&self, path: &str) -> AppResult<String> {
        fs::read_to_string(path)
            .map_err(|e| io_error("Failed to read rulebook", Path::new(path), e))
    }

    fn read_bytes(&self, path: &str) -> AppResult<Vec<u8>> {
        fs::read(path).map_err(|e| io_error("Failed to read rulebook", Path::new(path), e))
    }

    /// 룰북 디렉터리 바로 아래의 파일만 지운다. 다른 곳을 가리키는 경로는 건드리지 않는다.
    fn delete(&self, path: &str) -> AppResult<()> {
        let target = Path::new(path);
        if target.parent() != Some(self.rulebook_dir().as_path()) {
            tracing::warn!("Not deleting rulebook file outside rulebook dir: {path}");
            return Ok(());
        }
        match fs::remove_file(target) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
            Err(e) => Err(io_error("Failed to delete rulebook", target, e)),
        }
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
    fn storing_same_filename_repeatedly_keeps_every_file() {
        let temp = tempfile::tempdir().unwrap();
        let store = store_in(temp.path());

        let stored: Vec<_> = (0..20)
            .map(|n| {
                store
                    .store("rules.md", format!("content {n}").as_bytes())
                    .unwrap()
            })
            .collect();

        let distinct: std::collections::HashSet<_> = stored.iter().map(|s| &s.path).collect();
        assert_eq!(20, distinct.len());
        for (n, file) in stored.iter().enumerate() {
            assert_eq!(format!("content {n}"), store.read_text(&file.path).unwrap());
            assert!(file.path.ends_with("_rules.md"), "{}", file.path);
        }
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
    fn delete_removes_stored_file_and_ignores_missing_one() {
        let temp = tempfile::tempdir().unwrap();
        let store = store_in(temp.path());
        let kept = store.store("a.md", b"a").unwrap();
        let removed = store.store("a.md", b"b").unwrap();

        store.delete(&removed.path).unwrap();
        store.delete(&removed.path).unwrap();

        assert!(!Path::new(&removed.path).exists());
        assert_eq!("a", store.read_text(&kept.path).unwrap());
    }

    #[test]
    fn delete_leaves_files_outside_rulebook_directory() {
        let temp = tempfile::tempdir().unwrap();
        let store = store_in(&temp.path().join("data"));
        let outside = temp.path().join("keep.md");
        fs::write(&outside, "keep").unwrap();

        store.delete(&outside.to_string_lossy()).unwrap();

        assert!(outside.exists());
    }
}
