//! 백업 파일 쓰기/읽기. 대화상자는 `lib.rs`의 명령이 띄우고 고른 경로만 여기로 넘긴다.
//!
//! 백업은 웹뷰와 앱 사이를 JSON이 아닌 바이트 그대로 오간다. 저장할 때는 요청 본문이 백업 자체이고
//! 기본 파일 이름은 `BACKUP_NAME_HEADER`(퍼센트 인코딩)로 온다. 고른 파일은 이름 길이(u32, 리틀
//! 엔디언), UTF-8 이름, 파일 내용을 이어 붙인 바이트로 돌려준다(빈 바이트는 취소).
//!
//! 저장은 같은 폴더의 임시 파일에 다 쓰고 디스크에 내린 뒤 이름을 바꿔 덮어쓴다. 쓰다가 실패하면
//! (디스크가 가득 참 등) 원래 백업이 그대로 남는다. 가져올 파일은 Backend가 받는 크기
//! (`MAX_IMPORT_BYTES`)를 넘으면 읽지 않고 거절한다.

use std::borrow::Cow;
use std::io::Read;
use std::path::Path;

use tauri::ipc::InvokeBody;
use tokio::io::AsyncWriteExt;

pub const DEFAULT_BACKUP_NAME: &str = "omypic-backup.zip";
pub const FILTER_NAME: &str = "Zip Archive";
pub const FILTER_EXTENSIONS: &[&str] = &["zip"];
/// 저장 대화상자의 기본 파일 이름을 담는 요청 헤더.
pub const BACKUP_NAME_HEADER: &str = "x-omypic-backup-name";
/// 가져올 수 있는 백업의 최대 크기. Backend가 받는 요청 본문의 한도와 같다.
pub const MAX_IMPORT_BYTES: usize = omypic_presentation::MAX_BODY_BYTES;
const TEMP_PREFIX: &str = ".omypic-backup-";
const TEMP_SUFFIX: &str = ".tmp";

/// 저장 대화상자의 기본 파일 이름: 헤더 값의 마지막 경로 조각(`/`와 `\`를 모두 구분자로 본다).
/// 비었거나 `.`/`..`이면 `omypic-backup.zip`.
pub fn default_name(name: Option<String>) -> String {
    name.as_deref()
        .and_then(|value| value.rsplit(['/', '\\']).next())
        .map(str::trim)
        .filter(|value| !value.is_empty() && !matches!(*value, "." | ".."))
        .map(str::to_string)
        .unwrap_or_else(|| DEFAULT_BACKUP_NAME.to_string())
}

/// 헤더로 온 퍼센트 인코딩된 파일 이름. 잘못 인코딩됐으면 `None`(기본 이름을 쓴다).
pub fn decode_header_name(value: Option<&str>) -> Option<String> {
    let value = value?;
    let mut bytes = Vec::with_capacity(value.len());
    let mut rest = value.as_bytes();
    while let Some((&first, tail)) = rest.split_first() {
        if first == b'%' {
            let hex = std::str::from_utf8(tail.get(..2)?).ok()?;
            bytes.push(u8::from_str_radix(hex, 16).ok()?);
            rest = &tail[2..];
        } else {
            bytes.push(first);
            rest = tail;
        }
    }
    String::from_utf8(bytes).ok()
}

/// 저장할 백업 바이트. 보통은 요청 본문 그대로(복사하지 않는다). 웹뷰가 사용자 지정 프로토콜을
/// 쓰지 못해 postMessage로 보낸 요청은 숫자 배열(JSON)로 오므로 바꿔서 쓴다.
pub fn body_bytes(body: &InvokeBody) -> Result<Cow<'_, [u8]>, String> {
    match body {
        InvokeBody::Raw(bytes) => Ok(Cow::Borrowed(bytes)),
        InvokeBody::Json(serde_json::Value::Array(values)) => values
            .iter()
            .map(|value| {
                value
                    .as_u64()
                    .and_then(|byte| u8::try_from(byte).ok())
                    .ok_or_else(|| "The backup must be bytes".to_string())
            })
            .collect::<Result<Vec<u8>, String>>()
            .map(Cow::Owned),
        InvokeBody::Json(_) => Err("The backup must be bytes".to_string()),
    }
}

/// 고른 경로에 쓴다. 대화상자를 닫았으면(`None`) `false`. 바이트를 복사하지 않고 조각씩 쓴다.
pub async fn write_backup(path: Option<&Path>, bytes: &[u8]) -> Result<bool, String> {
    let Some(path) = path else {
        return Ok(false);
    };
    write_backup_from(path, bytes).await?;
    Ok(true)
}

/// `source`를 `path` 옆의 임시 파일에 다 쓰고 디스크에 내린 뒤 `path`로 이름을 바꾼다. 실패하면
/// 임시 파일을 지우고 `path`는 건드리지 않는다.
async fn write_backup_from(
    path: &Path,
    mut source: impl tokio::io::AsyncRead + Unpin,
) -> Result<(), String> {
    let failed = |err: std::io::Error| format!("Failed to write {}: {err}", path.display());
    let dir = match path.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir,
        _ => Path::new("."),
    };
    // 임시 파일은 놓이면(실패해 이 함수를 빠져나가면) 지워진다.
    let temp = tempfile::Builder::new()
        .prefix(TEMP_PREFIX)
        .suffix(TEMP_SUFFIX)
        .tempfile_in(dir)
        .map_err(failed)?;
    let mut file = tokio::fs::File::from_std(temp.as_file().try_clone().map_err(failed)?);
    tokio::io::copy(&mut source, &mut file)
        .await
        .map_err(failed)?;
    file.flush().await.map_err(failed)?;
    file.sync_all().await.map_err(failed)?;
    drop(file);
    temp.persist(path).map_err(|err| failed(err.error))?;
    Ok(())
}

/// 고른 파일을 읽어 이름과 내용을 이은 바이트로 돌려준다. 대화상자를 닫았으면(`None`) 빈 바이트.
/// 파일 내용은 한 번만 읽어 그 버퍼에 바로 담는다.
pub fn read_backup(path: Option<&Path>) -> Result<Vec<u8>, String> {
    let Some(path) = path else {
        return Ok(Vec::new());
    };
    let failed = |err: std::io::Error| format!("Failed to read {}: {err}", path.display());
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let name_length = u32::try_from(name.len()).map_err(|_| "File name too long".to_string())?;
    let file = std::fs::File::open(path).map_err(failed)?;
    let size = file.metadata().map_err(failed)?.len();
    let limit = MAX_IMPORT_BYTES as u64;
    if size > limit {
        return Err(too_large(size));
    }
    let capacity = usize::try_from(size)
        .unwrap_or(0)
        .saturating_add(4 + name.len());
    let mut out = Vec::with_capacity(capacity);
    out.extend_from_slice(&name_length.to_le_bytes());
    out.extend_from_slice(name.as_bytes());
    // 고른 뒤에 파일이 커졌어도 한도까지만 읽는다.
    let read = file.take(limit + 1).read_to_end(&mut out).map_err(failed)?;
    if read as u64 > limit {
        return Err(too_large(read as u64));
    }
    Ok(out)
}

/// 웹뷰가 "데이터 가져오기에 실패했습니다: …" 뒤에 그대로 보여 주는 글.
fn too_large(size: u64) -> String {
    const MIB: u64 = 1024 * 1024;
    format!(
        "백업 파일이 너무 큽니다({:.1} MiB). 가져올 수 있는 백업은 {} MiB까지입니다.",
        size as f64 / MIB as f64,
        MAX_IMPORT_BYTES as u64 / MIB
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `read_backup`의 바이트를 이름과 내용으로 나눈다(웹뷰 쪽 `readPickedFile`과 같은 규칙).
    fn split(reply: &[u8]) -> (String, Vec<u8>) {
        let length = u32::from_le_bytes(reply[..4].try_into().unwrap()) as usize;
        (
            String::from_utf8(reply[4..4 + length].to_vec()).unwrap(),
            reply[4 + length..].to_vec(),
        )
    }

    #[test]
    fn default_name_falls_back_to_backup_zip() {
        assert_eq!("omypic-backup.zip", default_name(None));
        assert_eq!("omypic-backup.zip", default_name(Some(" ".into())));
        assert_eq!("mine.zip", default_name(Some("mine.zip".into())));
    }

    #[test]
    fn header_names_are_percent_decoded() {
        assert_eq!(
            Some("omypic-backup-20261120-0910.zip".to_string()),
            decode_header_name(Some("omypic-backup-20261120-0910.zip"))
        );
        assert_eq!(
            Some("백업 1.zip".to_string()),
            decode_header_name(Some("%EB%B0%B1%EC%97%85%201.zip"))
        );
        assert_eq!(Some(String::new()), decode_header_name(Some("")));
        assert_eq!(None, decode_header_name(None));
        for broken in ["%", "%E", "%ZZ.zip", "%FF.zip"] {
            assert_eq!(None, decode_header_name(Some(broken)), "{broken}");
        }
    }

    #[test]
    fn raw_bodies_are_used_as_they_are() {
        let body = InvokeBody::Raw(vec![80, 75, 3]);
        let bytes = body_bytes(&body).unwrap();
        assert!(matches!(bytes, Cow::Borrowed(_)));
        assert_eq!(&[80, 75, 3], bytes.as_ref());
    }

    #[test]
    fn a_number_array_body_still_works_and_other_json_is_refused() {
        let body = InvokeBody::Json(serde_json::json!([80, 75, 255]));
        assert_eq!(&[80, 75, 255], body_bytes(&body).unwrap().as_ref());
        for json in [
            serde_json::json!({"bytes": [1]}),
            serde_json::json!([256]),
            serde_json::json!([-1]),
            serde_json::json!(["a"]),
        ] {
            assert!(
                body_bytes(&InvokeBody::Json(json.clone())).is_err(),
                "{json}"
            );
        }
    }

    #[tokio::test]
    async fn cancelled_dialogs_do_nothing() {
        assert_eq!(Ok(false), write_backup(None, b"zip").await);
        assert_eq!(Ok(Vec::new()), read_backup(None));
    }

    #[tokio::test]
    async fn written_backup_reads_back_with_file_name() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("백업.zip");
        let content: Vec<u8> = (0..3_000_000_u32).map(|n| (n % 251) as u8).collect();

        assert_eq!(Ok(true), write_backup(Some(&path), &content).await);
        let reply = read_backup(Some(&path)).unwrap();

        assert_eq!(("백업.zip".to_string(), content.clone()), split(&reply));
        assert_eq!(4 + "백업.zip".len() + content.len(), reply.len());
    }

    #[test]
    fn unreadable_backup_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read_backup(Some(&dir.path().join("missing.zip"))).is_err());
    }

    #[test]
    fn default_name_keeps_only_the_last_path_component() {
        assert_eq!("evil.zip", default_name(Some("../../evil.zip".into())));
        assert_eq!("b.zip", default_name(Some("a/b.zip".into())));
        assert_eq!("b.zip", default_name(Some("C:\\Users\\a\\b.zip".into())));
        assert_eq!("omypic-backup.zip", default_name(Some("dir/".into())));
        assert_eq!("omypic-backup.zip", default_name(Some("a/..".into())));
        assert_eq!("omypic-backup.zip", default_name(Some("/".into())));
    }

    #[test]
    fn an_import_file_larger_than_the_backend_accepts_is_refused_before_reading() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("huge.zip");
        // 성긴 파일이라 디스크를 쓰지 않는다.
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(omypic_presentation::MAX_BODY_BYTES as u64 + 1)
            .unwrap();
        drop(file);

        let Err(error) = read_backup(Some(&path)) else {
            panic!("a file over the limit was read");
        };
        assert!(error.contains("64 MiB"), "{error}");

        let fits = dir.path().join("fits.zip");
        std::fs::write(&fits, b"PK").unwrap();
        assert!(read_backup(Some(&fits)).is_ok());
    }

    /// `good`만큼 내준 뒤 읽기 오류(디스크가 가득 찬 것과 같은 실패)로 끝나는 원본.
    struct FailingSource {
        good: Vec<u8>,
    }

    impl tokio::io::AsyncRead for FailingSource {
        fn poll_read(
            mut self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
            buf: &mut tokio::io::ReadBuf<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            if self.good.is_empty() {
                return std::task::Poll::Ready(Err(std::io::Error::other(
                    "No space left on device",
                )));
            }
            let good = std::mem::take(&mut self.good);
            buf.put_slice(&good);
            std::task::Poll::Ready(Ok(()))
        }
    }

    #[tokio::test]
    async fn a_failed_export_leaves_the_previous_backup_intact_and_no_temporary_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("backup.zip");
        std::fs::write(&path, b"previous backup").unwrap();

        let error = write_backup_from(
            &path,
            FailingSource {
                good: b"half of the new".to_vec(),
            },
        )
        .await
        .unwrap_err();

        assert!(error.contains("No space left"), "{error}");
        assert_eq!(b"previous backup".to_vec(), std::fs::read(&path).unwrap());
        let names: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(vec![std::ffi::OsString::from("backup.zip")], names);

        assert_eq!(Ok(true), write_backup(Some(&path), b"new backup").await);
        assert_eq!(b"new backup".to_vec(), std::fs::read(&path).unwrap());
    }
}
