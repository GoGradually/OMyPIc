use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use omypic_application::rulebook::model::StoredRulebookFile;
use omypic_application::rulebook::port::RulebookFileStore;
use omypic_application::shared::policy::DataDirProvider;
use omypic_domain::error::{AppError, AppResult};
use uuid::Uuid;

/// 저장 이름이 우연히 겹칠 때 다시 시도하는 횟수.
const STORE_ATTEMPTS: usize = 8;
/// 데이터 폴더 안의 룰북 폴더 이름. 저장 경로는 `rulebooks/<이름>`이다.
const RULEBOOK_DIR: &str = "rulebooks";

/// 룰북 원본 파일을 `<data-dir>/rulebooks/` 아래에 저장한다.
pub struct FileSystemRulebookStore {
    data_dir_provider: Arc<dyn DataDirProvider>,
}

impl FileSystemRulebookStore {
    pub fn new(data_dir_provider: Arc<dyn DataDirProvider>) -> Self {
        Self { data_dir_provider }
    }

    fn data_dir(&self) -> PathBuf {
        PathBuf::from(self.data_dir_provider.get_data_dir())
    }

    fn rulebook_dir(&self) -> PathBuf {
        self.data_dir().join(RULEBOOK_DIR)
    }

    /// 저장된 경로를 실제 파일 경로로 바꾼다.
    ///
    /// - 상대 경로(`rulebooks/<이름>`, 지금 저장하는 꼴)는 데이터 폴더 기준이다. 폴더를 옮기거나
    ///   복원해도 그대로 맞는다. `..`처럼 데이터 폴더를 벗어나는 경로는 거절한다.
    /// - 절대 경로(0.3.0까지 저장하던 꼴)는 그대로 쓴다. 그 파일이 없으면(다른 곳에서 옮겨 온
    ///   데이터 폴더, 다른 OS의 경로) 이 룰북 폴더에 같은 이름의 파일이 있는지 본다.
    pub fn resolve(&self, path: &str) -> AppResult<PathBuf> {
        let given = Path::new(path);
        let direct = if is_foreign_absolute(path) {
            given.to_path_buf()
        } else {
            if !given
                .components()
                .all(|part| matches!(part, Component::Normal(_)))
            {
                return Err(AppError::InvalidArgument(format!(
                    "Rulebook path leaves the data folder: {path}"
                )));
            }
            self.data_dir().join(given)
        };
        if direct.exists() || !is_foreign_absolute(path) {
            return Ok(direct);
        }
        let name = path.rsplit(['/', '\\']).next().unwrap_or_default();
        if !name.is_empty() {
            let local = self.rulebook_dir().join(name);
            if local.exists() {
                return Ok(local);
            }
        }
        Ok(direct)
    }
}

/// 데이터 폴더 기준이 아닌 경로: 이 OS의 절대 경로나 다른 OS의 절대 경로(`C:\\…`, `\\…`, `/…`).
fn is_foreign_absolute(path: &str) -> bool {
    let bytes = path.as_bytes();
    Path::new(path).is_absolute()
        || path.starts_with(['/', '\\'])
        || (bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':')
}

fn read_error(path: &Path, error: std::io::Error) -> AppError {
    if error.kind() == ErrorKind::NotFound {
        AppError::NotFound(format!("Rulebook file is missing: {}", path.display()))
    } else {
        io_error("Failed to read rulebook", path, error)
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
            let stored_name = format!("{}_{name}", Uuid::new_v4().simple());
            let target = dir.join(&stored_name);
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
            // 데이터 폴더 기준 상대 경로로 돌려준다(폴더를 옮겨도 맞는다). 구분자는 OS와 관계없이 `/`.
            return Ok(StoredRulebookFile {
                path: format!("{RULEBOOK_DIR}/{stored_name}"),
            });
        }
        Err(AppError::Internal(format!(
            "Failed to find a free rulebook file name in {}",
            dir.display()
        )))
    }

    fn read_text(&self, path: &str) -> AppResult<String> {
        let target = self.resolve(path)?;
        fs::read_to_string(&target).map_err(|e| read_error(&target, e))
    }

    fn read_bytes(&self, path: &str) -> AppResult<Vec<u8>> {
        let target = self.resolve(path)?;
        fs::read(&target).map_err(|e| read_error(&target, e))
    }

    /// 룰북 디렉터리 바로 아래의 파일만 지운다. 다른 곳을 가리키는 경로는 건드리지 않는다.
    fn delete(&self, path: &str) -> AppResult<()> {
        let Ok(target) = self.resolve(path) else {
            tracing::warn!("Not deleting rulebook file outside the data folder: {path}");
            return Ok(());
        };
        let target = target.as_path();
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

        let path = store.resolve(&stored.path).unwrap();
        assert!(path.starts_with(temp.path().join("rulebooks")));
        assert!(path.exists());
        assert_eq!("hello markdown", fs::read_to_string(&path).unwrap());
    }

    #[test]
    fn stored_path_is_relative_to_the_data_folder() {
        let temp = tempfile::tempdir().unwrap();
        let store = store_in(temp.path());

        let stored = store.store("rules.md", b"x").unwrap();

        assert!(stored.path.starts_with("rulebooks/"), "{}", stored.path);
        assert!(Path::new(&stored.path).is_relative());
        assert!(temp.path().join(&stored.path).exists());
    }

    #[test]
    fn moved_data_folder_keeps_reading_and_deleting_stored_files() {
        let temp = tempfile::tempdir().unwrap();
        let old_dir = temp.path().join("old");
        let stored = store_in(&old_dir).store("rules.md", b"moved").unwrap();
        let new_dir = temp.path().join("new");
        fs::rename(&old_dir, &new_dir).unwrap();
        let store = store_in(&new_dir);

        assert_eq!("moved", store.read_text(&stored.path).unwrap());
        store.delete(&stored.path).unwrap();
        assert!(fs::read_dir(new_dir.join("rulebooks"))
            .unwrap()
            .next()
            .is_none());
    }

    #[test]
    fn absolute_path_from_another_data_folder_falls_back_to_this_rulebook_folder() {
        let temp = tempfile::tempdir().unwrap();
        let store = store_in(temp.path());
        fs::create_dir_all(temp.path().join("rulebooks")).unwrap();
        fs::write(
            temp.path().join("rulebooks/1700000000000_rules.md"),
            "restored",
        )
        .unwrap();
        let old = "/home/someone-else/OMyPIc/rulebooks/1700000000000_rules.md";

        assert_eq!("restored", store.read_text(old).unwrap());
        assert_eq!(b"restored".to_vec(), store.read_bytes(old).unwrap());
        store.delete(old).unwrap();
        assert!(!temp
            .path()
            .join("rulebooks/1700000000000_rules.md")
            .exists());
    }

    #[test]
    fn missing_file_is_reported_as_not_found() {
        let temp = tempfile::tempdir().unwrap();
        let store = store_in(temp.path());

        let error = store.read_bytes("rulebooks/gone.md").unwrap_err();

        assert!(matches!(error, AppError::NotFound(_)), "{error:?}");
    }

    #[test]
    fn relative_path_that_climbs_out_is_refused() {
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join("data");
        let store = store_in(&data);
        fs::write(temp.path().join("secret.md"), "secret").unwrap();

        assert!(store.read_text("../secret.md").is_err());
        store.delete("../secret.md").unwrap();
        assert!(temp.path().join("secret.md").exists());
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

        let path = store.resolve(&stored.path).unwrap();
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

        assert!(!store.resolve(&removed.path).unwrap().exists());
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
