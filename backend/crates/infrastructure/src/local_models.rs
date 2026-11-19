//! 온디바이스 음성 모델 파일을 상위 서버(기본 Hugging Face)에서 받아 데이터 폴더에 보관한다.
//!
//! 캐시 위치는 `<데이터 폴더>/local-models/<org>/<name>/<revision>/<file>`이다. 없는 파일은
//! 같은 폴더의 `*.part`에 받고, 다 받은 뒤에만 이름을 바꿔 넣으므로 일부만 받은 파일은 서빙되지
//! 않는다. 같은 파일을 동시에 요청하면 한 번만 받고 나머지는 그 결과를 기다린다.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use omypic_application::shared::local_model::{CachedModelFile, LocalModelError, LocalModelFiles};

use crate::config::LocalModelsConfig;

/// 데이터 폴더 안의 캐시 폴더 이름.
pub const LOCAL_MODELS_DIR: &str = "local-models";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
const HEADER_TIMEOUT: Duration = Duration::from_secs(60);
const STALL_TIMEOUT: Duration = Duration::from_secs(30);
const PART_SUFFIX: &str = ".part";

pub struct LocalModelCache {
    upstream: String,
    allowed_repos: Vec<String>,
    root: PathBuf,
    header_timeout: Duration,
    stall_timeout: Duration,
    in_flight: Mutex<HashMap<PathBuf, Arc<Flight>>>,
}

/// 받는 중인 파일 하나. 기다리는 쪽은 `result`가 채워질 때까지 자고, 실패도 그대로 받는다.
struct Flight {
    result: Mutex<Option<Result<(), LocalModelError>>>,
    done: Condvar,
}

impl LocalModelCache {
    pub fn new(config: &LocalModelsConfig, data_dir: &Path) -> Self {
        Self {
            upstream: config.upstream.trim_end_matches('/').to_string(),
            allowed_repos: config.allowed_repos.clone(),
            root: data_dir.join(LOCAL_MODELS_DIR),
            header_timeout: HEADER_TIMEOUT,
            stall_timeout: STALL_TIMEOUT,
            in_flight: Mutex::new(HashMap::new()),
        }
    }

    #[cfg(test)]
    fn with_timeouts(mut self, header_timeout: Duration, stall_timeout: Duration) -> Self {
        self.header_timeout = header_timeout;
        self.stall_timeout = stall_timeout;
        self
    }

    fn cached_path(&self, repo: &str, revision: &str, file: &str) -> PathBuf {
        let mut path = self.root.join(repo).join(revision);
        path.extend(file.split('/'));
        path
    }

    fn cached(path: PathBuf) -> CachedModelFile {
        CachedModelFile {
            content_type: content_type_of(&path).to_string(),
            path,
        }
    }

    fn flights(&self) -> MutexGuard<'_, HashMap<PathBuf, Arc<Flight>>> {
        self.in_flight
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// 이 파일을 받는 중이면 그 받기를, 아니면 이 호출이 받을 차례라는 표시를 돌려준다.
    /// 파일이 이미 있으면 `Ready`다.
    fn join_or_lead(&self, path: &Path) -> Turn<'_> {
        let mut flights = self.flights();
        if let Some(flight) = flights.get(path) {
            return Turn::Wait(flight.clone());
        }
        // 앞선 받기가 이름을 바꾼 직후에 온 요청이 다시 받지 않게, 등록하기 전에 한 번 더 본다.
        if path.is_file() {
            return Turn::Ready;
        }
        let flight = Arc::new(Flight {
            result: Mutex::new(None),
            done: Condvar::new(),
        });
        flights.insert(path.to_path_buf(), flight.clone());
        Turn::Lead(Leader {
            cache: self,
            path: path.to_path_buf(),
            flight,
            finished: false,
        })
    }

    fn download(&self, url: &str, target: &Path) -> Result<(), LocalModelError> {
        let dir = target
            .parent()
            .ok_or_else(|| internal("no parent folder"))?;
        std::fs::create_dir_all(dir)
            .map_err(|err| internal(format!("Failed to create {}: {err}", dir.display())))?;
        let (sender, receiver) = std::sync::mpsc::channel::<DownloadEvent>();
        let cancel = Arc::new(AtomicBool::new(false));
        let job = DownloadJob {
            url: url.to_string(),
            target: target.to_path_buf(),
            cancel: cancel.clone(),
        };
        std::thread::spawn(move || {
            let done = job.run(&sender);
            let _ = sender.send(DownloadEvent::Done(done));
        });

        // reqwest 블로킹 클라이언트에는 읽기 제한 시간이 없다. 받는 일은 별도 스레드가 하고, 여기서는
        // 응답 헤더를 기다리는 시간과 본문이 멈춘 시간을 잰다.
        let mut wait = self.header_timeout;
        loop {
            match receiver.recv_timeout(wait) {
                Ok(DownloadEvent::Started | DownloadEvent::Progress) => wait = self.stall_timeout,
                Ok(DownloadEvent::Done(result)) => return result,
                Err(_) => {
                    cancel.store(true, Ordering::SeqCst);
                    let what = if wait == self.header_timeout {
                        "timed out waiting for response headers"
                    } else {
                        "stalled: no data received"
                    };
                    return Err(LocalModelError::Upstream(format!(
                        "Model download failed: {what}"
                    )));
                }
            }
        }
    }
}

impl LocalModelFiles for LocalModelCache {
    fn fetch(
        &self,
        repo: &str,
        revision: &str,
        file: &str,
    ) -> Result<CachedModelFile, LocalModelError> {
        let (org, name) = repo
            .split_once('/')
            .ok_or_else(|| LocalModelError::InvalidPath("Invalid repository".to_string()))?;
        for segment in [org, name, revision].into_iter().chain(file.split('/')) {
            check_segment(segment)?;
        }
        if !self.allowed_repos.iter().any(|allowed| allowed == repo) {
            return Err(LocalModelError::NotAllowed(format!(
                "Repository '{repo}' is not allowed"
            )));
        }

        let path = self.cached_path(repo, revision, file);
        if path.is_file() {
            return Ok(Self::cached(path));
        }
        match self.join_or_lead(&path) {
            Turn::Ready => Ok(Self::cached(path)),
            Turn::Wait(flight) => {
                flight.wait()?;
                Ok(Self::cached(path))
            }
            Turn::Lead(mut leader) => {
                let url = format!("{}/{repo}/resolve/{revision}/{file}", self.upstream);
                let result = self.download(&url, &path);
                leader.finish(result.clone());
                result.map(|()| Self::cached(path))
            }
        }
    }
}

enum Turn<'a> {
    Ready,
    Wait(Arc<Flight>),
    Lead(Leader<'a>),
}

impl Flight {
    fn wait(&self) -> Result<(), LocalModelError> {
        let mut result = self.result.lock().unwrap_or_else(PoisonError::into_inner);
        loop {
            if let Some(outcome) = result.as_ref() {
                return outcome.clone();
            }
            result = self
                .done
                .wait(result)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }
}

/// 받는 일을 맡은 호출. 결과를 알리지 못하고 사라져도(패닉) 기다리는 쪽을 깨우고 등록을 지운다.
struct Leader<'a> {
    cache: &'a LocalModelCache,
    path: PathBuf,
    flight: Arc<Flight>,
    finished: bool,
}

impl Leader<'_> {
    fn finish(&mut self, result: Result<(), LocalModelError>) {
        if self.finished {
            return;
        }
        self.finished = true;
        // 등록을 먼저 지운다. 실패해도 다음 요청이 새로 시작할 수 있어야 한다.
        self.cache.flights().remove(&self.path);
        *self
            .flight
            .result
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(result);
        self.flight.done.notify_all();
    }
}

impl Drop for Leader<'_> {
    fn drop(&mut self) {
        self.finish(Err(internal("Model download aborted")));
    }
}

fn internal(message: impl Into<String>) -> LocalModelError {
    LocalModelError::Internal(message.into())
}

/// 비어 있지 않고, `.`/`..`이 아니고, `[A-Za-z0-9._-]+`이며, 임시 파일(`*.part`)이 아니어야 한다.
fn check_segment(segment: &str) -> Result<(), LocalModelError> {
    let valid = !matches!(segment, "" | "." | "..")
        && !segment.ends_with(PART_SUFFIX)
        && segment
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if valid {
        Ok(())
    } else {
        Err(LocalModelError::InvalidPath(
            "Invalid model path".to_string(),
        ))
    }
}

fn content_type_of(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("json") => "application/json",
        Some("wasm") => "application/wasm",
        Some("txt") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

enum DownloadEvent {
    /// 응답 헤더를 받았다.
    Started,
    /// 본문 조각을 받았다.
    Progress,
    Done(Result<(), LocalModelError>),
}

struct DownloadJob {
    url: String,
    target: PathBuf,
    cancel: Arc<AtomicBool>,
}

impl DownloadJob {
    fn run(&self, events: &std::sync::mpsc::Sender<DownloadEvent>) -> Result<(), LocalModelError> {
        let transfer = |err: &dyn std::fmt::Display| {
            LocalModelError::Upstream(format!("Model download failed: {err}"))
        };
        let client = reqwest::blocking::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(None)
            .tcp_keepalive(Some(Duration::from_secs(30)))
            .build()
            .map_err(|err| internal(format!("Failed to build HTTP client: {err}")))?;
        let mut response = client.get(&self.url).send().map_err(|err| transfer(&err))?;
        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(LocalModelError::NotFound(
                "Model file not found upstream".to_string(),
            ));
        }
        if !status.is_success() {
            return Err(LocalModelError::Upstream(format!(
                "Model download failed with status {}",
                status.as_u16()
            )));
        }
        let _ = events.send(DownloadEvent::Started);

        let dir = self.target.parent().unwrap_or_else(|| Path::new("."));
        let mut temp = tempfile::Builder::new()
            .prefix(".download-")
            .suffix(PART_SUFFIX)
            .tempfile_in(dir)
            .map_err(|err| internal(format!("Failed to create temporary file: {err}")))?;
        let write_error = |err: std::io::Error| internal(format!("Failed to write model: {err}"));
        let expected = response.content_length();
        let mut received = 0_u64;
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            if self.cancel.load(Ordering::SeqCst) {
                return Err(LocalModelError::Upstream(
                    "Model download cancelled".to_string(),
                ));
            }
            let read = response.read(&mut buffer).map_err(|err| transfer(&err))?;
            if read == 0 {
                break;
            }
            let _ = events.send(DownloadEvent::Progress);
            received += read as u64;
            temp.write_all(&buffer[..read]).map_err(write_error)?;
        }
        temp.flush().map_err(write_error)?;
        if expected.is_some_and(|expected| expected != received) {
            return Err(LocalModelError::Upstream(format!(
                "Model download was incomplete: got {received} of {} bytes",
                expected.unwrap_or_default()
            )));
        }
        if self.cancel.load(Ordering::SeqCst) {
            return Err(LocalModelError::Upstream(
                "Model download cancelled".to_string(),
            ));
        }
        temp.persist(&self.target)
            .map_err(|err| internal(format!("Failed to store model: {}", err.error)))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;

    const REPO: &str = "Xenova/whisper-base.en";

    fn config(upstream: &str) -> LocalModelsConfig {
        LocalModelsConfig {
            upstream: upstream.to_string(),
            allowed_repos: vec![REPO.to_string()],
        }
    }

    fn cache(upstream: &str, dir: &Path) -> LocalModelCache {
        LocalModelCache::new(&config(upstream), dir)
    }

    fn fetch(cache: &LocalModelCache, file: &str) -> Result<CachedModelFile, LocalModelError> {
        cache.fetch(REPO, "main", file)
    }

    fn files_under(dir: &Path) -> Vec<String> {
        fn walk(dir: &Path, out: &mut Vec<String>) {
            for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, out);
                } else {
                    out.push(path.display().to_string());
                }
            }
        }
        let mut out = Vec::new();
        walk(dir, &mut out);
        out
    }

    #[test]
    fn first_request_downloads_and_caches() {
        let dir = tempfile::tempdir().unwrap();
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/Xenova/whisper-base.en/resolve/main/onnx/encoder.onnx");
            then.status(200).body("model-bytes");
        });
        let cache = cache(&server.base_url(), dir.path());

        let first = fetch(&cache, "onnx/encoder.onnx").unwrap();
        let second = fetch(&cache, "onnx/encoder.onnx").unwrap();

        assert_eq!(1, mock.hits());
        assert_eq!(first, second);
        assert_eq!(
            dir.path()
                .join("local-models/Xenova/whisper-base.en/main/onnx/encoder.onnx"),
            first.path
        );
        assert_eq!(b"model-bytes".to_vec(), std::fs::read(&first.path).unwrap());
        assert_eq!(1, files_under(dir.path()).len(), "임시 파일이 남지 않는다");
    }

    #[test]
    fn upstream_with_trailing_slash_is_accepted() {
        let dir = tempfile::tempdir().unwrap();
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/Xenova/whisper-base.en/resolve/main/config.json");
            then.status(200).body("{}");
        });
        let cache = cache(&format!("{}/", server.base_url()), dir.path());

        fetch(&cache, "config.json").unwrap();

        assert_eq!(1, mock.hits());
    }

    /// 헤더에 `Content-Length: 100`을 쓰고 10바이트만 보낸 뒤 끊는 서버. 요청마다 한 번씩 답한다.
    fn truncating_server(requests: usize) -> (String, std::thread::JoinHandle<()>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            for _ in 0..requests {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = [0_u8; 2048];
                let _ = stream.read(&mut request);
                stream
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\n0123456789",
                    )
                    .unwrap();
            }
        });
        (url, handle)
    }

    #[test]
    fn partial_download_is_not_served() {
        let dir = tempfile::tempdir().unwrap();
        let (url, server) = truncating_server(2);
        let cache = cache(&url, dir.path());

        let first = fetch(&cache, "model.onnx");
        let second = fetch(&cache, "model.onnx");

        assert!(
            matches!(first, Err(LocalModelError::Upstream(_))),
            "{first:?}"
        );
        assert!(
            matches!(second, Err(LocalModelError::Upstream(_))),
            "실패한 뒤에도 다시 내려받는다: {second:?}"
        );
        server.join().unwrap();
        assert!(
            files_under(dir.path()).is_empty(),
            "완성 파일도 임시 파일도 없다"
        );
    }

    #[test]
    fn failed_download_does_not_block_a_retry() {
        let dir = tempfile::tempdir().unwrap();
        let server = MockServer::start();
        let mut failing = server.mock(|when, then| {
            when.method(GET)
                .path_matches(Regex::new("model.onnx$").unwrap());
            then.status(500);
        });
        let cache = cache(&server.base_url(), dir.path());
        assert!(fetch(&cache, "model.onnx").is_err());
        failing.delete();
        server.mock(|when, then| {
            when.method(GET)
                .path_matches(Regex::new("model.onnx$").unwrap());
            then.status(200).body("ok");
        });

        let file = fetch(&cache, "model.onnx").unwrap();

        assert_eq!(b"ok".to_vec(), std::fs::read(file.path).unwrap());
    }

    #[test]
    fn concurrent_requests_download_once() {
        let dir = tempfile::tempdir().unwrap();
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path_matches(Regex::new("model.onnx$").unwrap());
            then.status(200)
                .delay(Duration::from_millis(400))
                .body("shared-bytes");
        });
        let cache = Arc::new(cache(&server.base_url(), dir.path()));

        let handles: Vec<_> = (0..4)
            .map(|_| {
                let cache = cache.clone();
                std::thread::spawn(move || fetch(&cache, "model.onnx"))
            })
            .collect();
        let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

        assert_eq!(1, mock.hits());
        for result in results {
            let file = result.unwrap();
            assert_eq!(b"shared-bytes".to_vec(), std::fs::read(file.path).unwrap());
        }
    }

    #[test]
    fn concurrent_requests_share_a_failure_and_a_later_request_retries() {
        let dir = tempfile::tempdir().unwrap();
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path_matches(Regex::new("model.onnx$").unwrap());
            then.status(500).delay(Duration::from_millis(400));
        });
        let cache = Arc::new(cache(&server.base_url(), dir.path()));

        let handles: Vec<_> = (0..3)
            .map(|_| {
                let cache = cache.clone();
                std::thread::spawn(move || fetch(&cache, "model.onnx"))
            })
            .collect();
        for handle in handles {
            assert!(matches!(
                handle.join().unwrap(),
                Err(LocalModelError::Upstream(_))
            ));
        }
        assert_eq!(1, mock.hits(), "기다린 요청은 실패도 함께 받는다");

        assert!(fetch(&cache, "model.onnx").is_err());
        assert_eq!(2, mock.hits(), "실패가 다음 요청을 막지 않는다");
        assert!(cache.flights().is_empty());
    }

    #[test]
    fn rejects_repo_outside_allowlist() {
        let dir = tempfile::tempdir().unwrap();
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET);
            then.status(200).body("x");
        });
        let cache = cache(&server.base_url(), dir.path());

        for repo in [
            "evil/model",
            "xenova/whisper-base.en",
            "Xenova/whisper-base.en2",
        ] {
            let result = cache.fetch(repo, "main", "config.json");
            assert!(
                matches!(result, Err(LocalModelError::NotAllowed(_))),
                "{repo}: {result:?}"
            );
        }
        assert_eq!(0, mock.hits());
    }

    #[test]
    fn rejects_path_traversal_and_odd_segments() {
        let dir = tempfile::tempdir().unwrap();
        let cache = cache("http://127.0.0.1:1", dir.path());

        for (repo, revision, file) in [
            (REPO, "main", "../x"),
            (REPO, "main", "a/../x"),
            (REPO, "main", "./x"),
            (REPO, "main", ""),
            (REPO, "main", "a//b"),
            (REPO, "main", "a/"),
            (REPO, "main", "a\\b"),
            (REPO, "main", "a b"),
            (REPO, "main", "x.part"),
            (REPO, "main", "dir/.download-abc.part"),
            (REPO, "", "x"),
            (REPO, "..", "x"),
            (REPO, "a/b", "x"),
            ("../Xenova", "main", "x"),
            ("Xenova/..", "main", "x"),
            ("Xenova", "main", "x"),
            ("Xenova/whisper-base.en/extra", "main", "x"),
            ("/whisper-base.en", "main", "x"),
        ] {
            let result = cache.fetch(repo, revision, file);
            assert!(
                matches!(result, Err(LocalModelError::InvalidPath(_))),
                "{repo} {revision} {file}: {result:?}"
            );
        }
        assert!(!dir.path().join("local-models").exists());
    }

    #[test]
    fn upstream_404_maps_to_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET);
            then.status(404);
        });
        let cache = cache(&server.base_url(), dir.path());

        let result = fetch(&cache, "missing.json");

        assert!(
            matches!(result, Err(LocalModelError::NotFound(_))),
            "{result:?}"
        );
        assert!(files_under(dir.path()).is_empty());
    }

    #[test]
    fn upstream_500_maps_to_upstream_error() {
        let dir = tempfile::tempdir().unwrap();
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET);
            then.status(500);
        });
        let cache = cache(&server.base_url(), dir.path());

        let result = fetch(&cache, "config.json");

        assert!(
            matches!(result, Err(LocalModelError::Upstream(_))),
            "{result:?}"
        );
        assert!(files_under(dir.path()).is_empty());
    }

    #[test]
    fn unreachable_upstream_maps_to_upstream_error() {
        let dir = tempfile::tempdir().unwrap();
        let free_port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let cache = cache(&format!("http://127.0.0.1:{free_port}"), dir.path());

        let result = fetch(&cache, "config.json");

        assert!(
            matches!(result, Err(LocalModelError::Upstream(_))),
            "{result:?}"
        );
    }

    #[test]
    fn serves_wasm_and_onnx_content_types() {
        let dir = tempfile::tempdir().unwrap();
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET);
            then.status(200).body("x");
        });
        let cache = cache(&server.base_url(), dir.path());

        for (file, expected) in [
            ("config.json", "application/json"),
            ("onnx/model_quantized.onnx", "application/octet-stream"),
            ("voices/af_heart.bin", "application/octet-stream"),
            ("ort.wasm", "application/wasm"),
            ("merges.txt", "text/plain; charset=utf-8"),
            ("README", "application/octet-stream"),
        ] {
            assert_eq!(
                expected,
                fetch(&cache, file).unwrap().content_type,
                "{file}"
            );
        }
    }

    #[test]
    fn stalled_download_fails_after_the_stall_limit_and_leaves_no_file() {
        let dir = tempfile::tempdir().unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 2048];
            let _ = stream.read(&mut request);
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\n01234",
                )
                .unwrap();
            let _ = stream.flush();
            std::thread::sleep(Duration::from_secs(3));
        });
        let cache = cache(&url, dir.path())
            .with_timeouts(Duration::from_secs(5), Duration::from_millis(700));

        let result = fetch(&cache, "model.onnx");

        assert_eq!(
            Err(LocalModelError::Upstream(
                "Model download failed: stalled: no data received".to_string()
            )),
            result
        );
        server.join().unwrap();
        // 취소된 받기 스레드가 임시 파일을 지울 때까지 잠깐 기다린다.
        std::thread::sleep(Duration::from_millis(300));
        assert!(files_under(dir.path()).is_empty());
    }
}
