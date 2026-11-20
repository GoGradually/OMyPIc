//! 온디바이스 음성 모델 파일을 상위 서버(기본 Hugging Face)에서 받아 데이터 폴더에 보관한다.
//!
//! 캐시 위치는 `<데이터 폴더>/local-models/<org>/<name>/<revision>/<file>`이다. 없는 파일은
//! 같은 폴더의 `*.part`에 받고, 다 받은 뒤에만 이름을 바꿔 넣으므로 일부만 받은 파일은 캐시에
//! 들어가지 않는다.
//!
//! 받기는 요청한 쪽과 상관없이 별도 스레드에서 끝까지 진행된다. 요청한 쪽은 상위 서버의 응답
//! 헤더가 오는 대로 `.part`를 따라 읽는다(`Follower`). 같은 파일을 동시에 요청하면 한 번만 받고,
//! 나중 요청도 같은 `.part`를 처음부터 따라 읽는다. 읽는 쪽이 느리거나 사라져도 받기는 멈추지
//! 않는다. 받기가 실패하면 따라 읽던 쪽은 오류로 끝난다.
//!
//! 캐시에 없고 받는 중도 아닌 파일의 정보 요청(HEAD, `Range`)은 상위 서버에 같은 것을 물어
//! 답하고 아무것도 캐시하지 않는다. 앱이 받는 도중에 끝나 남은 임시 파일(`.download-*.part`)은
//! 캐시를 만들 때 지운다.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use omypic_application::shared::local_model::{
    CachedModelFile, DownloadingModelFile, LocalModelError, LocalModelFiles, ModelFile,
    ModelFileProbe, UpstreamModelReply,
};

use crate::config::LocalModelsConfig;

/// 데이터 폴더 안의 캐시 폴더 이름.
pub const LOCAL_MODELS_DIR: &str = "local-models";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
const HEADER_TIMEOUT: Duration = Duration::from_secs(60);
const STALL_TIMEOUT: Duration = Duration::from_secs(30);
/// 정보 요청(HEAD, `Range`)은 작으므로 본문까지 이 시간 안에 끝나야 한다.
const PROBE_TIMEOUT: Duration = Duration::from_secs(60);
const PART_SUFFIX: &str = ".part";
const PART_PREFIX: &str = ".download-";

type Flights = Arc<Mutex<HashMap<PathBuf, Arc<Flight>>>>;

pub struct LocalModelCache {
    upstream: String,
    allowed_repos: Vec<String>,
    root: PathBuf,
    header_timeout: Duration,
    stall_timeout: Duration,
    in_flight: Flights,
}

/// 받는 중인 파일 하나. 받기 스레드가 상태를 바꿀 때마다 `changed`로 깨운다.
struct Flight {
    state: Mutex<FlightState>,
    changed: Condvar,
}

struct FlightState {
    phase: Phase,
    /// `.part`에 써 둔 바이트 수.
    written: u64,
}

enum Phase {
    /// 상위 서버의 응답 헤더를 기다린다.
    Connecting,
    /// 본문을 `part`에 쓰는 중이다.
    Receiving { part: PathBuf, length: Option<u64> },
    /// 끝났다. `Ok`이면 캐시 파일이 제자리에 있다.
    Finished(Result<(), LocalModelError>),
}

impl LocalModelCache {
    pub fn new(config: &LocalModelsConfig, data_dir: &Path) -> Self {
        let root = data_dir.join(LOCAL_MODELS_DIR);
        remove_stale_downloads(&root);
        Self {
            upstream: config.upstream.trim_end_matches('/').to_string(),
            allowed_repos: config.allowed_repos.clone(),
            root,
            header_timeout: HEADER_TIMEOUT,
            stall_timeout: STALL_TIMEOUT,
            in_flight: Arc::new(Mutex::new(HashMap::new())),
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
        lock(&self.in_flight)
    }

    /// 이 파일을 받는 중이면 그 받기를, 캐시에 있으면 `Ready`를, 정보 요청이면 `Probe`를 돌려준다.
    /// 그 밖에는 받기를 시작하고 그 받기를 돌려준다.
    fn join_or_start(&self, path: &Path, url: &str, probe: bool) -> Result<Turn, LocalModelError> {
        let mut flights = self.flights();
        if let Some(flight) = flights.get(path) {
            return Ok(Turn::Follow(flight.clone()));
        }
        // 앞선 받기가 이름을 바꾼 직후에 온 요청이 다시 받지 않게, 등록하기 전에 한 번 더 본다.
        if path.is_file() {
            return Ok(Turn::Ready);
        }
        if probe {
            return Ok(Turn::Probe);
        }
        let dir = path.parent().ok_or_else(|| internal("no parent folder"))?;
        std::fs::create_dir_all(dir)
            .map_err(|err| internal(format!("Failed to create {}: {err}", dir.display())))?;
        let flight = Arc::new(Flight {
            state: Mutex::new(FlightState {
                phase: Phase::Connecting,
                written: 0,
            }),
            changed: Condvar::new(),
        });
        flights.insert(path.to_path_buf(), flight.clone());
        drop(flights);

        let supervisor = Supervisor {
            flights: self.in_flight.clone(),
            path: path.to_path_buf(),
            flight: flight.clone(),
            header_timeout: self.header_timeout,
            stall_timeout: self.stall_timeout,
            finished: false,
        };
        let job = DownloadJob {
            url: url.to_string(),
            target: path.to_path_buf(),
            flight: flight.clone(),
            cancel: Arc::new(AtomicBool::new(false)),
        };
        std::thread::spawn(move || supervisor.run(job));
        Ok(Turn::Follow(flight))
    }

    /// 받기의 응답 헤더가 올 때까지 기다린 뒤 따라 읽을 본문을 돌려준다.
    fn follow(&self, flight: Arc<Flight>, path: PathBuf) -> Result<ModelFile, LocalModelError> {
        let mut state = flight.state();
        while matches!(state.phase, Phase::Connecting) {
            state = flight
                .changed
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
        let (file, length) = match &state.phase {
            Phase::Connecting | Phase::Finished(Ok(())) => {
                return Ok(ModelFile::Cached(Self::cached(path)));
            }
            Phase::Finished(Err(err)) => return Err(err.clone()),
            // 잠금을 쥔 채 연다. 받기가 끝나 이름이 바뀌는 일도 이 잠금 안에서 일어나므로 여기서 연
            // 파일은 이름이 바뀐 뒤에도 같은 파일이다.
            Phase::Receiving { part, length } => (
                std::fs::File::open(part)
                    .map_err(|err| internal(format!("Failed to open {}: {err}", part.display())))?,
                *length,
            ),
        };
        drop(state);
        Ok(ModelFile::Downloading(DownloadingModelFile {
            content_type: content_type_of(&path).to_string(),
            length,
            body: Box::new(Follower {
                file,
                flight,
                offset: 0,
            }),
        }))
    }

    /// 캐시에 없는 파일의 정보 요청을 상위 서버에 그대로 묻는다.
    fn ask_upstream(
        &self,
        url: &str,
        path: &Path,
        probe: &ModelFileProbe,
    ) -> Result<ModelFile, LocalModelError> {
        let client = reqwest::blocking::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(PROBE_TIMEOUT)
            .build()
            .map_err(|err| internal(format!("Failed to build HTTP client: {err}")))?;
        let method = if probe.head {
            reqwest::Method::HEAD
        } else {
            reqwest::Method::GET
        };
        let mut request = client.request(method, url);
        if let Some(range) = &probe.range {
            request = request.header(reqwest::header::RANGE, range);
        }
        let response = request
            .send()
            .map_err(|err| LocalModelError::Upstream(format!("Model download failed: {err}")))?;
        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(LocalModelError::NotFound(
                "Model file not found upstream".to_string(),
            ));
        }
        let satisfiable = status != reqwest::StatusCode::RANGE_NOT_SATISFIABLE;
        if !status.is_success() && satisfiable {
            return Err(LocalModelError::Upstream(format!(
                "Model download failed with status {}",
                status.as_u16()
            )));
        }
        let content_range = response
            .headers()
            .get(reqwest::header::CONTENT_RANGE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let content_length = response
            .headers()
            .get(reqwest::header::CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok());
        // HEAD와 416에는 보낼 본문이 없다. 응답은 여기(블로킹 스레드)에서 버린다.
        let (content_length, body): (Option<u64>, Box<dyn Read + Send>) = if probe.head {
            (content_length, Box::new(std::io::empty()))
        } else if !satisfiable {
            (None, Box::new(std::io::empty()))
        } else {
            (content_length, Box::new(response))
        };
        Ok(ModelFile::Upstream(UpstreamModelReply {
            status: status.as_u16(),
            content_type: content_type_of(path).to_string(),
            content_length,
            content_range,
            body,
        }))
    }
}

impl LocalModelFiles for LocalModelCache {
    fn open(
        &self,
        repo: &str,
        revision: &str,
        file: &str,
        probe: Option<&ModelFileProbe>,
    ) -> Result<ModelFile, LocalModelError> {
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
            return Ok(ModelFile::Cached(Self::cached(path)));
        }
        let url = format!("{}/{repo}/resolve/{revision}/{file}", self.upstream);
        match self.join_or_start(&path, &url, probe.is_some())? {
            Turn::Ready => Ok(ModelFile::Cached(Self::cached(path))),
            Turn::Follow(flight) => self.follow(flight, path),
            Turn::Probe => {
                let probe = probe.expect("Turn::Probe only for probes");
                self.ask_upstream(&url, &path, probe)
            }
        }
    }
}

enum Turn {
    Ready,
    Follow(Arc<Flight>),
    Probe,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Flight {
    fn state(&self) -> MutexGuard<'_, FlightState> {
        lock(&self.state)
    }

    /// 아직 끝나지 않았으면 `result`로 끝내고 기다리는 쪽을 깨운다. 실패면 `.part`도 지운다
    /// (멈춘 받기 스레드가 아직 파일을 쥐고 있어도 지금 지운다).
    fn finish(&self, result: Result<(), LocalModelError>) {
        let mut state = self.state();
        if matches!(state.phase, Phase::Finished(_)) {
            return;
        }
        if let Phase::Receiving { part, .. } = &state.phase {
            if result.is_err() {
                let _ = std::fs::remove_file(part);
            }
        }
        state.phase = Phase::Finished(result);
        drop(state);
        self.changed.notify_all();
    }
}

/// 받는 중인 `.part`를 처음부터 읽는다. 써 둔 만큼만 읽고, 더 없으면 받기 스레드가 더 쓸 때까지
/// 기다린다. 받기가 성공하면 끝(0)을, 실패하면 오류를 돌려준다.
struct Follower {
    file: std::fs::File,
    flight: Arc<Flight>,
    offset: u64,
}

impl Read for Follower {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let mut state = self.flight.state();
        let available = loop {
            if let Phase::Finished(Err(err)) = &state.phase {
                return Err(std::io::Error::other(err.clone()));
            }
            let available = state.written - self.offset;
            if available > 0 {
                break available;
            }
            if matches!(state.phase, Phase::Finished(Ok(()))) {
                return Ok(0);
            }
            state = self
                .flight
                .changed
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        };
        drop(state);
        let wanted = buf
            .len()
            .min(usize::try_from(available).unwrap_or(usize::MAX));
        let read = self.file.read(&mut buf[..wanted])?;
        if read == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "model download file is shorter than written",
            ));
        }
        self.offset += read as u64;
        Ok(read)
    }
}

/// 받기 하나를 지켜본다. 응답 헤더가 늦거나 본문이 멈추면 받기를 실패로 끝낸다. 이 스레드가
/// 결과를 알리지 못하고 사라져도(패닉) 기다리는 쪽을 깨우고 등록을 지운다.
struct Supervisor {
    flights: Flights,
    path: PathBuf,
    flight: Arc<Flight>,
    header_timeout: Duration,
    stall_timeout: Duration,
    finished: bool,
}

impl Supervisor {
    fn run(mut self, job: DownloadJob) {
        let (sender, receiver) = std::sync::mpsc::channel::<DownloadEvent>();
        let cancel = job.cancel.clone();
        std::thread::spawn(move || {
            let done = job.run(&sender);
            let _ = sender.send(DownloadEvent::Done(done));
        });
        let result = self.watch(&receiver);
        if result.is_err() {
            cancel.store(true, Ordering::SeqCst);
        }
        self.finish(result);
    }

    // reqwest 블로킹 클라이언트에는 읽기 제한 시간이 없다. 받는 일은 별도 스레드가 하고, 여기서는
    // 응답 헤더를 기다리는 시간과 본문이 멈춘 시간을 잰다. 읽는 쪽(Follower)은 받기를 막지 않는다.
    fn watch(&self, events: &Receiver<DownloadEvent>) -> Result<(), LocalModelError> {
        let mut wait = self.header_timeout;
        loop {
            match events.recv_timeout(wait) {
                Ok(DownloadEvent::Started | DownloadEvent::Progress) => wait = self.stall_timeout,
                Ok(DownloadEvent::Done(result)) => return result,
                Err(RecvTimeoutError::Timeout) => {
                    let what = if wait == self.header_timeout {
                        "timed out waiting for response headers"
                    } else {
                        "stalled: no data received"
                    };
                    return Err(LocalModelError::Upstream(format!(
                        "Model download failed: {what}"
                    )));
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(internal("Model download aborted"));
                }
            }
        }
    }

    fn finish(&mut self, result: Result<(), LocalModelError>) {
        if self.finished {
            return;
        }
        self.finished = true;
        // 등록을 먼저 지운다. 실패해도 다음 요청이 새로 시작할 수 있어야 한다.
        lock(&self.flights).remove(&self.path);
        if let Err(err) = &result {
            tracing::warn!(error = %err, path = %self.path.display(), "model download failed");
        }
        self.flight.finish(result);
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        self.finish(Err(internal("Model download aborted")));
    }
}

/// `dir` 아래(하위 폴더 포함)에서 받다 만 임시 파일(`.download-*.part`)을 지운다. 다 받은 파일은
/// 이 이름을 가질 수 없다(`check_segment`가 `*.part`를 거부한다). 실패는 경고만 남긴다.
fn remove_stale_downloads(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            remove_stale_downloads(&path);
            continue;
        }
        let stale = kind.is_file()
            && entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with(PART_PREFIX) && name.ends_with(PART_SUFFIX));
        if stale {
            match std::fs::remove_file(&path) {
                Ok(()) => {
                    tracing::info!("Removed an unfinished model download: {}", path.display())
                }
                Err(err) => tracing::warn!("Cannot remove {}: {err}", path.display()),
            }
        }
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
    flight: Arc<Flight>,
    cancel: Arc<AtomicBool>,
}

impl DownloadJob {
    fn cancelled() -> LocalModelError {
        LocalModelError::Upstream("Model download cancelled".to_string())
    }

    fn run(&self, events: &Sender<DownloadEvent>) -> Result<(), LocalModelError> {
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

        let dir = self.target.parent().unwrap_or_else(|| Path::new("."));
        let mut temp = tempfile::Builder::new()
            .prefix(PART_PREFIX)
            .suffix(PART_SUFFIX)
            .tempfile_in(dir)
            .map_err(|err| internal(format!("Failed to create temporary file: {err}")))?;
        let write_error = |err: std::io::Error| internal(format!("Failed to write model: {err}"));
        let expected = response.content_length();
        {
            let mut state = self.flight.state();
            if matches!(state.phase, Phase::Finished(_)) {
                return Err(Self::cancelled());
            }
            state.phase = Phase::Receiving {
                part: temp.path().to_path_buf(),
                length: expected,
            };
        }
        self.flight.changed.notify_all();
        let _ = events.send(DownloadEvent::Started);

        let mut received = 0_u64;
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            if self.cancel.load(Ordering::SeqCst) {
                return Err(Self::cancelled());
            }
            let read = response.read(&mut buffer).map_err(|err| transfer(&err))?;
            if read == 0 {
                break;
            }
            let _ = events.send(DownloadEvent::Progress);
            temp.write_all(&buffer[..read]).map_err(write_error)?;
            received += read as u64;
            self.flight.state().written = received;
            self.flight.changed.notify_all();
        }
        temp.flush().map_err(write_error)?;
        if expected.is_some_and(|expected| expected != received) {
            return Err(LocalModelError::Upstream(format!(
                "Model download was incomplete: got {received} of {} bytes",
                expected.unwrap_or_default()
            )));
        }
        // 이름 바꾸기와 완료 표시를 한 잠금 안에서 한다. 감시 스레드가 먼저 실패로 끝냈으면
        // (멈춤 판정) 캐시에 넣지 않는다.
        let mut state = self.flight.state();
        if self.cancel.load(Ordering::SeqCst) || matches!(state.phase, Phase::Finished(_)) {
            return Err(Self::cancelled());
        }
        temp.persist(&self.target)
            .map_err(|err| internal(format!("Failed to store model: {}", err.error)))?;
        state.phase = Phase::Finished(Ok(()));
        drop(state);
        self.flight.changed.notify_all();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;

    const REPO: &str = "Xenova/whisper-base.en";
    /// 비워 둔 임시 포트는 함께 도는 다른 시험의 가짜 서버가 다시 잡을 수 있다. 1번은 듣는 쪽이 없다.
    const UNREACHABLE_UPSTREAM: &str = "http://127.0.0.1:1";

    fn config(upstream: &str) -> LocalModelsConfig {
        LocalModelsConfig {
            upstream: upstream.to_string(),
            allowed_repos: vec![REPO.to_string()],
        }
    }

    fn cache(upstream: &str, dir: &Path) -> LocalModelCache {
        LocalModelCache::new(&config(upstream), dir)
    }

    /// 파일을 끝까지 읽은 뒤 캐시된 파일을 돌려준다(받는 중이면 다 받을 때까지 따라 읽는다).
    fn fetch(cache: &LocalModelCache, file: &str) -> Result<CachedModelFile, LocalModelError> {
        match cache.open(REPO, "main", file, None)? {
            ModelFile::Cached(cached) => Ok(cached),
            ModelFile::Downloading(mut download) => {
                let mut bytes = Vec::new();
                download
                    .body
                    .read_to_end(&mut bytes)
                    .map_err(model_error_of)?;
                let path = cache.cached_path(REPO, "main", file);
                assert_eq!(
                    bytes,
                    std::fs::read(&path).unwrap(),
                    "읽은 본문이 캐시 파일과 같다"
                );
                Ok(CachedModelFile {
                    content_type: download.content_type,
                    path,
                })
            }
            ModelFile::Upstream(reply) => panic!("not a probe: {}", reply.status),
        }
    }

    fn model_error_of(err: std::io::Error) -> LocalModelError {
        err.get_ref()
            .and_then(|inner| inner.downcast_ref::<LocalModelError>())
            .cloned()
            .unwrap_or_else(|| panic!("not a model error: {err}"))
    }

    fn probe(
        cache: &LocalModelCache,
        file: &str,
        head: bool,
        range: Option<&str>,
    ) -> Result<ModelFile, LocalModelError> {
        let probe = ModelFileProbe {
            head,
            range: range.map(str::to_string),
        };
        cache.open(REPO, "main", file, Some(&probe))
    }

    fn downloading(file: ModelFile) -> DownloadingModelFile {
        match file {
            ModelFile::Downloading(download) => download,
            other => panic!("expected a download in progress: {other:?}"),
        }
    }

    fn open_within(cache: &Arc<LocalModelCache>, probe: Option<ModelFileProbe>) -> ModelFile {
        let cache = cache.clone();
        within(Duration::from_secs(5), move || {
            cache.open(REPO, "main", "model.onnx", probe.as_ref())
        })
        .unwrap()
    }

    /// 다른 스레드에서 `job`을 돌리고 `limit` 안에 끝나지 않으면 실패한다(시험이 멈추지 않게).
    fn within<T: Send + 'static>(limit: Duration, job: impl FnOnce() -> T + Send + 'static) -> T {
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(job());
        });
        receiver
            .recv_timeout(limit)
            .unwrap_or_else(|_| panic!("did not finish within {limit:?}"))
    }

    type Body = Box<dyn Read + Send>;

    fn read_exactly(mut body: Body, count: usize) -> (Body, Vec<u8>) {
        within(Duration::from_secs(5), move || {
            let mut bytes = vec![0_u8; count];
            body.read_exact(&mut bytes).unwrap();
            (body, bytes)
        })
    }

    fn read_rest(mut body: Body) -> std::io::Result<Vec<u8>> {
        within(Duration::from_secs(5), move || {
            let mut bytes = Vec::new();
            body.read_to_end(&mut bytes).map(|_| bytes)
        })
    }

    fn wait_until(what: &str, check: impl Fn() -> bool) {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !check() {
            assert!(
                std::time::Instant::now() < deadline,
                "timed out waiting: {what}"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// 시험이 열 때까지 막는 문.
    #[derive(Clone, Default)]
    struct Gate(Arc<(Mutex<bool>, Condvar)>);

    impl Gate {
        fn open(&self) {
            *self.0 .0.lock().unwrap() = true;
            self.0 .1.notify_all();
        }

        fn wait(&self) {
            let mut open = self.0 .0.lock().unwrap();
            while !*open {
                open = self.0 .1.wait(open).unwrap();
            }
        }
    }

    /// 요청마다 `respond`로 직접 답하는 상위 서버. 받은 요청의 헤더 부분을 소문자로 기록한다.
    struct FakeUpstream {
        url: String,
        requests: Arc<Mutex<Vec<String>>>,
    }

    impl FakeUpstream {
        fn start(respond: impl Fn(&str, &mut std::net::TcpStream) + Send + Sync + 'static) -> Self {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://{}", listener.local_addr().unwrap());
            let requests = Arc::new(Mutex::new(Vec::new()));
            let recorded = requests.clone();
            let respond = Arc::new(respond);
            std::thread::spawn(move || {
                for stream in listener.incoming() {
                    let Ok(mut stream) = stream else { return };
                    let recorded = recorded.clone();
                    let respond = respond.clone();
                    std::thread::spawn(move || {
                        let mut head = Vec::new();
                        let mut byte = [0_u8; 1];
                        while !head.ends_with(b"\r\n\r\n") {
                            match stream.read(&mut byte) {
                                Ok(1) => head.push(byte[0]),
                                _ => return,
                            }
                        }
                        let head = String::from_utf8_lossy(&head).to_ascii_lowercase();
                        recorded.lock().unwrap().push(head.clone());
                        respond(&head, &mut stream);
                    });
                }
            });
            Self { url, requests }
        }

        fn requests(&self) -> Vec<String> {
            self.requests.lock().unwrap().clone()
        }
    }

    /// `Content-Length: 10`으로 `01234`를 보내고, 문이 열리면 `then`을 보내고 끊는다.
    fn split_upstream(gate: &Gate, then: &'static [u8]) -> FakeUpstream {
        let gate = gate.clone();
        FakeUpstream::start(move |_, stream| {
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\nConnection: close\r\n\r\n01234",
                )
                .unwrap();
            stream.flush().unwrap();
            gate.wait();
            let _ = stream.write_all(then);
        })
    }

    fn cache_file(dir: &Path, file: &str) -> PathBuf {
        dir.join("local-models/Xenova/whisper-base.en/main")
            .join(file)
    }

    fn upstream_error(result: &std::io::Result<Vec<u8>>) -> Option<LocalModelError> {
        result
            .as_ref()
            .err()
            .and_then(|err| err.get_ref())
            .and_then(|inner| inner.downcast_ref::<LocalModelError>())
            .cloned()
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
    fn creating_the_cache_removes_stale_downloads_and_keeps_finished_files() {
        let dir = tempfile::tempdir().unwrap();
        let repo_dir = dir.path().join("local-models/Xenova/whisper-base.en/main");
        std::fs::create_dir_all(repo_dir.join("onnx")).unwrap();
        // 앱이 받는 도중에 끝나면 `.download-*.part`가 남는다(`process::exit`는 임시 파일을 지우지 않는다).
        let stale = [
            repo_dir.join(".download-a1B2c3.part"),
            repo_dir.join("onnx/.download-Zz9.part"),
        ];
        let kept = [
            repo_dir.join("config.json"),
            repo_dir.join("onnx/encoder.onnx"),
            // 이름이 비슷해도 받기 임시 파일 규칙이 아니면 건드리지 않는다.
            repo_dir.join("download-x.part"),
            repo_dir.join(".download-notes.txt"),
        ];
        for path in stale.iter().chain(&kept) {
            std::fs::write(path, "x").unwrap();
        }
        let outside = dir.path().join(".download-other.part");
        std::fs::write(&outside, "x").unwrap();

        let _cache = cache("http://127.0.0.1:9", dir.path());

        for path in &stale {
            assert!(!path.exists(), "{} should be removed", path.display());
        }
        for path in kept.iter().chain([&outside]) {
            assert!(path.exists(), "{} should be kept", path.display());
        }
    }

    #[test]
    fn creating_the_cache_without_a_cache_folder_is_fine() {
        let dir = tempfile::tempdir().unwrap();
        let _cache = cache("http://127.0.0.1:9", dir.path());
        assert!(files_under(dir.path()).is_empty());
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
            for probe in [None, Some(&ModelFileProbe::default())] {
                let result = cache.open(repo, "main", "config.json", probe);
                assert!(
                    matches!(result, Err(LocalModelError::NotAllowed(_))),
                    "{repo}: {result:?}"
                );
            }
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
            for probe in [None, Some(&ModelFileProbe::default())] {
                let result = cache.open(repo, revision, file, probe);
                assert!(
                    matches!(result, Err(LocalModelError::InvalidPath(_))),
                    "{repo} {revision} {file}: {result:?}"
                );
            }
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
        let cache = cache(UNREACHABLE_UPSTREAM, dir.path());

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

    #[test]
    fn headers_and_first_bytes_arrive_before_the_upstream_finishes() {
        let dir = tempfile::tempdir().unwrap();
        let gate = Gate::default();
        let upstream = split_upstream(&gate, b"56789");
        let cache = Arc::new(cache(&upstream.url, dir.path()));

        let download = downloading(open_within(&cache, None));
        assert_eq!(Some(10), download.length);
        assert_eq!("application/octet-stream", download.content_type);
        let (body, first) = read_exactly(download.body, 5);
        assert_eq!(b"01234".to_vec(), first);
        assert!(
            !cache_file(dir.path(), "model.onnx").exists(),
            "아직 캐시에 없다"
        );

        gate.open();
        assert_eq!(b"56789".to_vec(), read_rest(body).unwrap());
        assert_eq!(
            b"0123456789".to_vec(),
            std::fs::read(cache_file(dir.path(), "model.onnx")).unwrap()
        );
        assert_eq!(1, files_under(dir.path()).len(), "임시 파일이 남지 않는다");
        assert_eq!(1, upstream.requests().len());
    }

    #[test]
    fn a_reader_that_goes_away_does_not_stop_the_download() {
        let dir = tempfile::tempdir().unwrap();
        let gate = Gate::default();
        let upstream = split_upstream(&gate, b"56789");
        let cache = Arc::new(cache(&upstream.url, dir.path()));

        let (body, _) = read_exactly(downloading(open_within(&cache, None)).body, 5);
        drop(body);
        gate.open();

        let target = cache_file(dir.path(), "model.onnx");
        wait_until("the file is cached", || target.is_file());
        assert_eq!(b"0123456789".to_vec(), std::fs::read(&target).unwrap());
        wait_until("no temporary file", || files_under(dir.path()).len() == 1);
        assert!(fetch(&cache, "model.onnx").is_ok());
        assert_eq!(
            1,
            upstream.requests().len(),
            "캐시된 파일은 다시 받지 않는다"
        );
    }

    #[test]
    fn failure_mid_download_ends_the_body_with_an_error_and_caches_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let gate = Gate::default();
        let upstream = split_upstream(&gate, b"");
        let cache = Arc::new(cache(&upstream.url, dir.path()));

        let (body, first) = read_exactly(downloading(open_within(&cache, None)).body, 5);
        assert_eq!(b"01234".to_vec(), first);
        gate.open();

        let rest = read_rest(body);
        assert!(
            matches!(upstream_error(&rest), Some(LocalModelError::Upstream(_))),
            "{rest:?}"
        );
        assert!(
            files_under(dir.path()).is_empty(),
            "완성 파일도 임시 파일도 없다"
        );
        assert!(cache.flights().is_empty());
        assert!(fetch(&cache, "model.onnx").is_err());
        assert_eq!(2, upstream.requests().len(), "다음 요청은 다시 받는다");
    }

    #[test]
    fn stall_mid_download_ends_the_body_with_an_error_and_caches_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let gate = Gate::default();
        let upstream = split_upstream(&gate, b"56789");
        let cache = Arc::new(
            cache(&upstream.url, dir.path())
                .with_timeouts(Duration::from_secs(5), Duration::from_millis(300)),
        );

        let (body, _) = read_exactly(downloading(open_within(&cache, None)).body, 5);
        let rest = read_rest(body);
        gate.open();

        assert_eq!(
            Some(LocalModelError::Upstream(
                "Model download failed: stalled: no data received".to_string()
            )),
            upstream_error(&rest)
        );
        // 멈춘 받기 스레드가 끝나기 전에도 임시 파일은 이미 없다.
        assert!(
            files_under(dir.path()).is_empty(),
            "{:?}",
            files_under(dir.path())
        );
        std::thread::sleep(Duration::from_millis(200));
        assert!(
            files_under(dir.path()).is_empty(),
            "늦게 끝난 받기가 파일을 넣지 않는다"
        );
    }

    #[test]
    fn a_second_request_follows_the_download_in_progress() {
        let dir = tempfile::tempdir().unwrap();
        let gate = Gate::default();
        let upstream = split_upstream(&gate, b"56789");
        let cache = Arc::new(cache(&upstream.url, dir.path()));

        let first = downloading(open_within(&cache, None));
        let second = downloading(open_within(&cache, None));
        assert_eq!(Some(10), second.length);
        let (second_body, second_start) = read_exactly(second.body, 5);
        assert_eq!(b"01234".to_vec(), second_start);
        gate.open();

        assert_eq!(b"0123456789".to_vec(), read_rest(first.body).unwrap());
        assert_eq!(b"56789".to_vec(), read_rest(second_body).unwrap());
        assert_eq!(1, upstream.requests().len(), "한 번만 받는다");
    }

    #[test]
    fn a_slow_reader_does_not_stall_the_download() {
        let dir = tempfile::tempdir().unwrap();
        let body: Vec<u8> = (0..2_000_000_u32).map(|n| (n % 251) as u8).collect();
        let sent = body.clone();
        let upstream = FakeUpstream::start(move |_, stream| {
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                sent.len()
            );
            stream.write_all(head.as_bytes()).unwrap();
            for piece in sent.chunks(100_000) {
                stream.write_all(piece).unwrap();
                std::thread::sleep(Duration::from_millis(10));
            }
        });
        // 상위 서버는 쉬지 않고 보내고, 읽는 쪽만 멈춤 한도보다 오래 쉰다.
        let cache = Arc::new(
            cache(&upstream.url, dir.path())
                .with_timeouts(Duration::from_secs(5), Duration::from_millis(500)),
        );

        let (reader, first) = read_exactly(downloading(open_within(&cache, None)).body, 1);
        std::thread::sleep(Duration::from_millis(1500));
        let mut received = first;
        received.extend(read_rest(reader).unwrap());

        assert_eq!(body, received);
        assert_eq!(
            body,
            std::fs::read(cache_file(dir.path(), "model.onnx")).unwrap()
        );
    }

    #[test]
    fn range_probe_of_an_uncached_file_asks_upstream_and_caches_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let upstream = FakeUpstream::start(|_, stream| {
            stream
                .write_all(b"HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 0-0/10\r\nContent-Length: 1\r\nConnection: close\r\n\r\n0")
                .unwrap();
        });
        let cache = cache(&upstream.url, dir.path());

        let reply = match probe(&cache, "model.onnx", false, Some("bytes=0-0")).unwrap() {
            ModelFile::Upstream(reply) => reply,
            other => panic!("{other:?}"),
        };

        assert_eq!(206, reply.status);
        assert_eq!(Some(1), reply.content_length);
        assert_eq!(Some("bytes 0-0/10".to_string()), reply.content_range);
        assert_eq!("application/octet-stream", reply.content_type);
        assert_eq!(b"0".to_vec(), read_rest(reply.body).unwrap());
        let requests = upstream.requests();
        assert_eq!(1, requests.len());
        assert!(requests[0].starts_with("get "), "{}", requests[0]);
        assert!(
            requests[0].contains("\r\nrange: bytes=0-0\r\n"),
            "{}",
            requests[0]
        );
        assert!(
            files_under(dir.path()).is_empty(),
            "정보 요청은 캐시에 아무것도 남기지 않는다"
        );
    }

    #[test]
    fn head_probe_of_an_uncached_file_asks_upstream_with_head() {
        let dir = tempfile::tempdir().unwrap();
        let upstream = FakeUpstream::start(|_, stream| {
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\nConnection: close\r\n\r\n")
                .unwrap();
        });
        let cache = cache(&upstream.url, dir.path());

        let reply = match probe(&cache, "model.onnx", true, None).unwrap() {
            ModelFile::Upstream(reply) => reply,
            other => panic!("{other:?}"),
        };

        assert_eq!(200, reply.status);
        assert_eq!(Some(10), reply.content_length);
        assert!(read_rest(reply.body).unwrap().is_empty());
        let requests = upstream.requests();
        assert_eq!(1, requests.len());
        assert!(requests[0].starts_with("head "), "{}", requests[0]);
        assert!(files_under(dir.path()).is_empty());
    }

    #[test]
    fn probe_failures_map_like_downloads() {
        let dir = tempfile::tempdir().unwrap();
        let upstream = FakeUpstream::start(|head, stream| {
            let reply: &[u8] = if head.contains("missing") {
                b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            } else if head.contains("broken") {
                b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            } else {
                b"HTTP/1.1 416 Range Not Satisfiable\r\nContent-Range: bytes */0\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            };
            stream.write_all(reply).unwrap();
        });
        let cache = cache(&upstream.url, dir.path());

        let missing = probe(&cache, "missing.json", false, Some("bytes=0-0"));
        let broken = probe(&cache, "broken.json", true, None);
        let empty = probe(&cache, "empty.txt", false, Some("bytes=0-0"));

        assert!(
            matches!(missing, Err(LocalModelError::NotFound(_))),
            "{missing:?}"
        );
        assert!(
            matches!(broken, Err(LocalModelError::Upstream(_))),
            "{broken:?}"
        );
        match empty.unwrap() {
            ModelFile::Upstream(reply) => {
                assert_eq!(416, reply.status);
                assert_eq!(Some("bytes */0".to_string()), reply.content_range);
            }
            other => panic!("{other:?}"),
        }
        assert!(files_under(dir.path()).is_empty());
    }

    #[test]
    fn unreachable_upstream_fails_a_probe_as_upstream_error() {
        let dir = tempfile::tempdir().unwrap();
        let cache = cache(UNREACHABLE_UPSTREAM, dir.path());

        let result = probe(&cache, "config.json", false, Some("bytes=0-0"));

        assert!(
            matches!(result, Err(LocalModelError::Upstream(_))),
            "{result:?}"
        );
    }

    #[test]
    fn probe_of_a_cached_file_makes_no_upstream_request() {
        let dir = tempfile::tempdir().unwrap();
        let upstream = FakeUpstream::start(|_, stream| {
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\nabc")
                .unwrap();
        });
        let cache = cache(&upstream.url, dir.path());
        fetch(&cache, "model.onnx").unwrap();

        let head = probe(&cache, "model.onnx", true, None).unwrap();
        let range = probe(&cache, "model.onnx", false, Some("bytes=0-0")).unwrap();

        assert!(matches!(head, ModelFile::Cached(_)), "{head:?}");
        assert!(matches!(range, ModelFile::Cached(_)), "{range:?}");
        assert_eq!(1, upstream.requests().len());
    }

    #[test]
    fn probe_during_a_download_follows_it_without_asking_upstream() {
        let dir = tempfile::tempdir().unwrap();
        let gate = Gate::default();
        let upstream = split_upstream(&gate, b"56789");
        let cache = Arc::new(cache(&upstream.url, dir.path()));
        let first = downloading(open_within(&cache, None));

        let probed = downloading(open_within(
            &cache,
            Some(ModelFileProbe {
                head: false,
                range: Some("bytes=0-0".to_string()),
            }),
        ));

        assert_eq!(Some(10), probed.length);
        assert_eq!(1, upstream.requests().len());
        gate.open();
        assert_eq!(b"0123456789".to_vec(), read_rest(first.body).unwrap());
    }
}
