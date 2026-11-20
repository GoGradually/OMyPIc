//! 온디바이스 음성 모델 파일을 상위 서버(기본 Hugging Face)에서 받아 데이터 폴더에 보관한다.
//!
//! 캐시 위치는 `<데이터 폴더>/local-models/<org>/<name>/<revision>/<file>`이다. 없는 파일은
//! 같은 폴더의 `*.part`에 받고, 다 받은 뒤에만 이름을 바꿔 넣으므로 일부만 받은 파일은 캐시에
//! 들어가지 않는다.
//!
//! 받기는 요청한 쪽과 상관없이 별도 스레드 하나에서 끝까지 진행된다. 요청한 쪽은 상위 서버의
//! 응답 헤더가 오는 대로 `.part`를 따라 읽는다(`Follower`). 같은 파일을 동시에 요청하면 한 번만
//! 받고, 나중 요청도 같은 `.part`를 처음부터 따라 읽는다. 읽는 쪽이 느리거나 사라져도 받기는
//! 멈추지 않는다. 받기가 실패하면 따라 읽던 쪽은 오류로 끝난다.
//!
//! 응답 헤더가 `HEADER_TIMEOUT` 안에 오지 않거나 본문 사이에 `STALL_TIMEOUT` 동안 아무것도 오지
//! 않으면 받기가 실패로 끝나므로 받기 스레드는 언제나 스스로 끝나고, 임시 파일과 연결도 그때 함께
//! 닫힌다. 받기 스레드는 자기 단일 스레드 런타임에서 비동기 클라이언트를 돌리고, 끝날 때 런타임과
//! 함께 연결을 닫는다.
//!
//! HTTP 클라이언트(와 그 안에 읽어 둔 OS 인증서 저장소)는 캐시마다 하나를 처음 쓸 때 만들어 모든
//! 받기와 정보 요청이 나눠 쓴다. 요청마다 런타임이 다르므로 연결은 요청 사이에 남겨 두지 않는다.
//!
//! 캐시에 없고 받는 중도 아닌 파일의 정보 요청(HEAD, 라이브러리가 보내는 `Range: bytes=0-0`)은
//! 상위 서버에 같은 것을 물어 답하고 아무것도 캐시하지 않는다. 그 밖의 `Range` 요청은 보통 GET처럼
//! 받기를 시작한다. 앱이 받는 도중에 끝나 남은 임시 파일(`.download-*.part`)은 캐시를 만들 때
//! 지운다.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use omypic_application::shared::local_model::{
    CachedModelFile, DownloadingModelFile, LocalModelError, LocalModelFiles, ModelFile,
    ModelFileProbe, UpstreamModelReply,
};
use tempfile::NamedTempFile;

use crate::config::LocalModelsConfig;

/// 데이터 폴더 안의 캐시 폴더 이름.
pub const LOCAL_MODELS_DIR: &str = "local-models";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
/// 요청을 보내고 응답 헤더가 올 때까지 기다리는 시간. Hugging Face는 CDN 주소로 재전송하므로 TLS
/// 연결 두 번이 이 안에 들어간다(HTTPS를 검사하는 프록시 뒤에서는 느리다).
const HEADER_TIMEOUT: Duration = Duration::from_secs(60);
/// 응답 헤더 뒤 본문 조각 사이에 기다리는 최대 시간.
const STALL_TIMEOUT: Duration = Duration::from_secs(30);
/// 정보 요청(HEAD, `Range: bytes=0-0`)은 연결부터 본문 끝까지 이 시간 안에 끝나야 한다.
const PROBE_TIMEOUT: Duration = Duration::from_secs(60);
/// 정보 요청의 답으로 옮겨 줄 본문의 한도. `bytes=0-0`의 답은 1바이트다.
const PROBE_BODY_LIMIT: u64 = 64 * 1024;
/// 라이브러리가 파일 크기를 알아보려고 보내는 `Range` 값. 이것만 상위 서버에 그대로 묻는다.
const FIRST_BYTE_RANGE: &str = "bytes=0-0";
const PART_SUFFIX: &str = ".part";
const PART_PREFIX: &str = ".download-";
/// 경로 조각 하나의 최대 바이트 수(파일 시스템의 이름 한도 255보다 작게).
const MAX_SEGMENT_BYTES: usize = 128;
/// `<org>/<name>/<revision>/<file>` 전체의 최대 바이트 수.
const MAX_RELATIVE_PATH_BYTES: usize = 512;
const DOWNLOAD_THREAD_NAME: &str = "omypic-model-download";

type Flights = Arc<Mutex<HashMap<PathBuf, Arc<Flight>>>>;

pub struct LocalModelCache {
    upstream: String,
    allowed_repos: Vec<String>,
    root: PathBuf,
    header_timeout: Duration,
    stall_timeout: Duration,
    in_flight: Flights,
    /// 살아 있는 받기 스레드 수.
    live_downloads: Arc<AtomicUsize>,
    /// 처음 쓸 때 만드는 HTTP 클라이언트. 비동기 클라이언트라 놓을 때 기다리는 스레드가 없다.
    client: Mutex<Option<reqwest::Client>>,
    /// 클라이언트를 만든 횟수(시험용).
    client_builds: AtomicUsize,
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
    /// 끝났다. `Ok`이면 캐시 파일이 제자리에 있고, `Err`이면 `.part`는 이미 지워졌다.
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
            live_downloads: Arc::default(),
            client: Mutex::new(None),
            client_builds: AtomicUsize::new(0),
        }
    }

    #[cfg(test)]
    fn with_idle_timeout(self, idle_timeout: Duration) -> Self {
        self.with_timeouts(idle_timeout, idle_timeout)
    }

    #[cfg(test)]
    fn with_timeouts(mut self, header: Duration, stall: Duration) -> Self {
        self.header_timeout = header;
        self.stall_timeout = stall;
        self
    }

    #[cfg(test)]
    fn client_builds(&self) -> usize {
        self.client_builds.load(Ordering::SeqCst)
    }

    #[cfg(test)]
    fn live_downloads(&self) -> usize {
        self.live_downloads.load(Ordering::SeqCst)
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

    /// 이 캐시의 HTTP 클라이언트. 처음 부를 때(요청을 처리하는 블로킹 스레드에서) 만든다. 동시에
    /// 처음 부른 요청들은 잠금에서 기다렸다가 같은 클라이언트를 받는다.
    fn client(&self) -> Result<reqwest::Client, LocalModelError> {
        let mut client = lock(&self.client);
        if let Some(client) = client.as_ref() {
            return Ok(client.clone());
        }
        self.client_builds.fetch_add(1, Ordering::SeqCst);
        let built = crate::http_client::build_async_client(|| {
            reqwest::Client::builder()
                .connect_timeout(CONNECT_TIMEOUT)
                .tcp_keepalive(Some(Duration::from_secs(30)))
                // 연결은 그 요청을 돌린 런타임에 묶이고 런타임은 요청마다 다르다.
                .pool_max_idle_per_host(0)
        })
        .map_err(|err| internal(format!("Failed to build HTTP client: {err}")))?;
        Ok(client.insert(built).clone())
    }

    /// 이 파일을 받는 중이면 그 받기를, 캐시에 있으면 `Ready`를, 정보 요청이면 `Probe`를 돌려준다.
    /// 그 밖에는 받기를 시작하고 그 받기를 돌려준다. 등록 잠금을 쥔 채로는 파일 시스템을 건드리지
    /// 않는다.
    fn join_or_start(&self, path: &Path, url: &str, probe: bool) -> Result<Turn, LocalModelError> {
        if let Some(flight) = self.flights().get(path) {
            return Ok(Turn::Follow(flight.clone()));
        }
        if path.is_file() {
            return Ok(Turn::Ready);
        }
        if probe {
            return Ok(Turn::Probe);
        }
        let flight = {
            let mut flights = self.flights();
            // 위에서 본 뒤에 다른 요청이 시작했을 수 있다.
            if let Some(flight) = flights.get(path) {
                return Ok(Turn::Follow(flight.clone()));
            }
            let flight = Arc::new(Flight::new());
            flights.insert(path.to_path_buf(), flight.clone());
            flight
        };
        let registration = Registration {
            flights: self.in_flight.clone(),
            path: path.to_path_buf(),
            flight: flight.clone(),
            finished: false,
        };
        // 위에서 본 뒤 등록하기 전에 앞선 받기가 끝나 이름을 바꿨으면 다시 받지 않는다.
        if path.is_file() {
            registration.finish(Ok(()), None);
            return Ok(Turn::Ready);
        }

        let client = match self.client() {
            Ok(client) => client,
            Err(err) => {
                registration.finish(Err(err.clone()), None);
                return Err(err);
            }
        };
        let job = DownloadJob {
            url: url.to_string(),
            target: path.to_path_buf(),
            flight: flight.clone(),
            client,
            header_timeout: self.header_timeout,
            stall_timeout: self.stall_timeout,
        };
        let live = LiveDownload::new(self.live_downloads.clone());
        let spawned = std::thread::Builder::new()
            .name(DOWNLOAD_THREAD_NAME.to_string())
            .spawn(move || {
                let _live = live;
                job.run(registration);
            });
        // 스레드를 띄우지 못하면 `registration`이 클로저와 함께 버려져 받기가 실패로 끝난다.
        spawned.map_err(|err| internal(format!("Failed to start the model download: {err}")))?;
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
            // 잠금을 쥔 채 연다. `.part`를 지우는 일(실패)과 이름을 바꾸는 일(성공)은 상태를
            // `Finished`로 바꾸는 이 잠금 안에서만 일어나므로, `Receiving`이면 `.part`가 있고 여기서
            // 연 파일은 이름이 바뀐 뒤에도 같은 파일이다.
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

    /// 캐시에 없는 파일의 정보 요청을 상위 서버에 묻는다. 본문(작다)은 여기서 다 읽어 두므로
    /// 상위 서버의 응답은 이 블로킹 스레드에서 닫힌다. 상위 서버가 범위를 무시하고 전체 파일로
    /// 답하면 `None`: 보통 GET처럼 받는다. 연결부터 본문 끝까지 `PROBE_TIMEOUT` 안에 끝나야 한다.
    fn ask_upstream(
        &self,
        url: &str,
        path: &Path,
        probe: &ModelFileProbe,
    ) -> Result<Option<ModelFile>, LocalModelError> {
        let client = self.client()?;
        let runtime = current_thread_runtime()?;
        let asked = runtime.block_on(async {
            tokio::time::timeout(PROBE_TIMEOUT, ask_upstream(&client, url, path, probe)).await
        });
        drop(runtime);
        asked.unwrap_or_else(|_| Err(upstream_failed("timed out")))
    }
}

async fn ask_upstream(
    client: &reqwest::Client,
    url: &str,
    path: &Path,
    probe: &ModelFileProbe,
) -> Result<Option<ModelFile>, LocalModelError> {
    let method = if probe.head {
        reqwest::Method::HEAD
    } else {
        reqwest::Method::GET
    };
    let mut request = client.request(method, url);
    if probe.range.as_deref().is_some_and(is_first_byte_range) {
        request = request.header(reqwest::header::RANGE, FIRST_BYTE_RANGE);
    }
    let mut response = request.send().await.map_err(upstream_failed)?;
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
    if !probe.head && status == reqwest::StatusCode::OK {
        return Ok(None);
    }
    let content_range = response
        .headers()
        .get(reqwest::header::CONTENT_RANGE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    // HEAD와 416에는 보낼 본문이 없다.
    let (content_length, body) = if probe.head {
        // HEAD의 `content_length()`는 본문 크기(0)라서 헤더를 직접 읽는다.
        let length = response
            .headers()
            .get(reqwest::header::CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok());
        (length, Vec::new())
    } else if !satisfiable {
        (None, Vec::new())
    } else {
        let too_large =
            || LocalModelError::Upstream("Model probe reply is larger than expected".to_string());
        if response
            .content_length()
            .is_some_and(|length| length > PROBE_BODY_LIMIT)
        {
            return Err(too_large());
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(upstream_failed)? {
            body.extend_from_slice(&chunk);
            if body.len() as u64 > PROBE_BODY_LIMIT {
                return Err(too_large());
            }
        }
        (Some(body.len() as u64), body)
    };
    drop(response);
    Ok(Some(ModelFile::Upstream(UpstreamModelReply {
        status: status.as_u16(),
        content_type: content_type_of(path).to_string(),
        content_length,
        content_range,
        body: Box::new(std::io::Cursor::new(body)),
    })))
}

/// 받기 하나나 정보 요청 하나를 돌리는 런타임. 놓으면 그 요청의 연결도 닫힌다.
fn current_thread_runtime() -> Result<tokio::runtime::Runtime, LocalModelError> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|err| internal(format!("Failed to start the model download: {err}")))
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
        check_path(org, name, revision, file)?;
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
        // 상위 서버에 그대로 묻는 것은 HEAD와 첫 바이트 요청뿐이다. 다른 `Range`는 보통 GET처럼
        // 받기를 시작하고, 라우트가 받는 파일 안에서 범위를 답한다.
        let probe = probe
            .filter(|probe| probe.head || probe.range.as_deref().is_some_and(is_first_byte_range));
        match self.join_or_start(&path, &url, probe.is_some())? {
            Turn::Ready => Ok(ModelFile::Cached(Self::cached(path))),
            Turn::Follow(flight) => self.follow(flight, path),
            Turn::Probe => {
                let probe = probe.expect("Turn::Probe only for probes");
                if let Some(reply) = self.ask_upstream(&url, &path, probe)? {
                    return Ok(reply);
                }
                match self.join_or_start(&path, &url, false)? {
                    Turn::Ready | Turn::Probe => Ok(ModelFile::Cached(Self::cached(path))),
                    Turn::Follow(flight) => self.follow(flight, path),
                }
            }
        }
    }
}

enum Turn {
    Ready,
    Follow(Arc<Flight>),
    Probe,
}

fn is_first_byte_range(value: &str) -> bool {
    value.trim() == FIRST_BYTE_RANGE
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Flight {
    fn new() -> Self {
        Self {
            state: Mutex::new(FlightState {
                phase: Phase::Connecting,
                written: 0,
            }),
            changed: Condvar::new(),
        }
    }

    fn state(&self) -> MutexGuard<'_, FlightState> {
        lock(&self.state)
    }
}

/// 받기 하나의 등록. 받기 스레드가 갖고 있다가 결과와 함께 `finish`한다. 결과를 알리지 못하고
/// 버려져도(패닉, 스레드를 띄우지 못함) 받기를 실패로 끝낸다.
struct Registration {
    flights: Flights,
    path: PathBuf,
    flight: Arc<Flight>,
    finished: bool,
}

impl Registration {
    fn finish(mut self, result: Result<(), LocalModelError>, part: Option<NamedTempFile>) {
        self.finished = true;
        finish_flight(&self.flights, &self.path, &self.flight, result, part);
    }
}

impl Drop for Registration {
    fn drop(&mut self) {
        if !self.finished {
            finish_flight(
                &self.flights,
                &self.path,
                &self.flight,
                Err(internal("Model download aborted")),
                None,
            );
        }
    }
}

/// 받기를 `result`로 끝낸다. 등록을 먼저 지우므로 이 뒤에 온 요청은 새로 받는다. 이미 이 받기를
/// 따라온 요청은 같은 결과를 받는다: 실패면 `.part`를 지우는 일과 `Finished`로 바꾸는 일을 이
/// 받기의 잠금 안에서 함께 하므로, `Receiving`을 본 요청은 언제나 `.part`를 열 수 있다.
fn finish_flight(
    flights: &Flights,
    path: &Path,
    flight: &Arc<Flight>,
    result: Result<(), LocalModelError>,
    part: Option<NamedTempFile>,
) {
    {
        let mut flights = lock(flights);
        if flights
            .get(path)
            .is_some_and(|registered| Arc::ptr_eq(registered, flight))
        {
            flights.remove(path);
        }
    }
    if let Err(err) = &result {
        tracing::warn!(error = %err, path = %path.display(), "model download failed");
    }
    let mut state = flight.state();
    if !matches!(state.phase, Phase::Finished(_)) {
        if result.is_err() {
            if let Some(part) = part {
                let _ = part.close();
            }
        }
        state.phase = Phase::Finished(result);
    }
    drop(state);
    flight.changed.notify_all();
}

/// 살아 있는 받기 스레드 하나를 센다.
struct LiveDownload(Arc<AtomicUsize>);

impl LiveDownload {
    fn new(count: Arc<AtomicUsize>) -> Self {
        count.fetch_add(1, Ordering::SeqCst);
        Self(count)
    }
}

impl Drop for LiveDownload {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
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

fn invalid_path() -> LocalModelError {
    LocalModelError::InvalidPath("Invalid model path".to_string())
}

/// 저장소, 리비전, 파일 경로의 모든 조각과 전체 길이를 검사한다.
fn check_path(org: &str, name: &str, revision: &str, file: &str) -> Result<(), LocalModelError> {
    let total = org.len() + name.len() + revision.len() + file.len() + 3;
    if total > MAX_RELATIVE_PATH_BYTES {
        return Err(invalid_path());
    }
    for segment in [org, name, revision].into_iter().chain(file.split('/')) {
        check_segment(segment)?;
    }
    Ok(())
}

/// 비어 있지 않고, `.`/`..`이 아니고, `[A-Za-z0-9._-]+`이고, `MAX_SEGMENT_BYTES` 이하이며, 다음이
/// 아니어야 한다: 임시 파일(`*.part`, 대소문자 무관: Windows와 macOS의 파일 시스템은 대소문자를
/// 가리지 않는다), `.`으로 끝나는 이름(Windows가 끝의 `.`을 떼어 다른 이름과 같아진다), Windows
/// 장치 이름(`con`, `nul`, `com0`~`com9`, `lpt0.json` 등).
fn check_segment(segment: &str) -> Result<(), LocalModelError> {
    let lower = segment.to_ascii_lowercase();
    let valid = !matches!(segment, "" | "." | "..")
        && segment.len() <= MAX_SEGMENT_BYTES
        && !lower.ends_with(PART_SUFFIX)
        && !segment.ends_with('.')
        && !is_windows_device_name(&lower)
        && segment
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if valid {
        Ok(())
    } else {
        Err(invalid_path())
    }
}

/// Windows가 확장자와 상관없이 장치로 여는 이름인가(`lower`는 소문자).
fn is_windows_device_name(lower: &str) -> bool {
    let stem = lower.split('.').next().unwrap_or_default();
    match stem {
        "con" | "prn" | "aux" | "nul" => true,
        _ => {
            let numbered = stem
                .strip_prefix("com")
                .or_else(|| stem.strip_prefix("lpt"));
            numbered.is_some_and(|digit| {
                matches!(
                    digit,
                    "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9"
                )
            })
        }
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

struct DownloadJob {
    url: String,
    target: PathBuf,
    flight: Arc<Flight>,
    client: reqwest::Client,
    header_timeout: Duration,
    stall_timeout: Duration,
}

impl DownloadJob {
    /// 받고 나서 결과로 받기를 끝낸다. 상위 서버의 응답과 연결(런타임과 함께)은 결과를 알리기 전에
    /// 이 스레드에서 닫는다.
    fn run(self, registration: Registration) {
        let mut part = None;
        let result = match current_thread_runtime() {
            Ok(runtime) => {
                let result = runtime.block_on(self.transfer(&mut part));
                drop(runtime);
                result
            }
            Err(err) => Err(err),
        };
        drop(self.client);
        registration.finish(result, part);
    }

    /// 실패하면 `part`에 받다 만 임시 파일을 남겨 둔다(`finish`가 지운다).
    async fn transfer(&self, part: &mut Option<NamedTempFile>) -> Result<(), LocalModelError> {
        let mut response =
            tokio::time::timeout(self.header_timeout, self.client.get(&self.url).send())
                .await
                .map_err(|_| upstream_failed("timed out waiting for response headers"))?
                .map_err(upstream_failed)?;
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

        // 폴더는 상위 서버가 파일을 주겠다고 답한 뒤에 만든다(404에 빈 폴더를 남기지 않는다).
        let dir = self.target.parent().unwrap_or_else(|| Path::new("."));
        std::fs::create_dir_all(dir)
            .map_err(|err| internal(format!("Failed to create {}: {err}", dir.display())))?;
        let temp = part.insert(
            tempfile::Builder::new()
                .prefix(PART_PREFIX)
                .suffix(PART_SUFFIX)
                .tempfile_in(dir)
                .map_err(|err| internal(format!("Failed to create temporary file: {err}")))?,
        );
        let write_error = |err: std::io::Error| internal(format!("Failed to write model: {err}"));
        let expected = response.content_length();
        self.flight.state().phase = Phase::Receiving {
            part: temp.path().to_path_buf(),
            length: expected,
        };
        self.flight.changed.notify_all();

        let mut received = 0_u64;
        loop {
            let chunk = tokio::time::timeout(self.stall_timeout, response.chunk())
                .await
                .map_err(|_| upstream_failed("stalled: no data received"))?
                .map_err(upstream_failed)?;
            let Some(chunk) = chunk else {
                break;
            };
            temp.write_all(&chunk).map_err(write_error)?;
            received += chunk.len() as u64;
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
        // 이름을 바꾸기 전에 내용을 디스크에 내린다. 그러지 않으면 전원이 꺼진 뒤 이름만 남은 빈
        // 파일이나 잘린 파일이 캐시에 들어가고, 캐시는 크기를 보지 않으므로 계속 그 파일을 준다.
        temp.as_file().sync_all().map_err(write_error)?;
        let stored = temp.as_file().metadata().map_err(write_error)?.len();
        if stored != received {
            return Err(internal(format!(
                "Model file was not stored completely: {stored} of {received} bytes"
            )));
        }
        drop(response);
        // 이름 바꾸기와 완료 표시를 한 잠금 안에서 한다(`follow`가 이것을 믿는다).
        let mut state = self.flight.state();
        let temp = part.take().expect("the temporary file was created above");
        match temp.persist(&self.target) {
            Ok(_) => {
                state.phase = Phase::Finished(Ok(()));
                Ok(())
            }
            Err(err) => {
                *part = Some(err.file);
                Err(internal(format!("Failed to store model: {}", err.error)))
            }
        }
    }
}

fn upstream_failed(reason: impl std::fmt::Display) -> LocalModelError {
    LocalModelError::Upstream(format!("Model download failed: {reason}"))
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
        let cache = cache(&url, dir.path()).with_idle_timeout(Duration::from_millis(700));

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
            cache(&upstream.url, dir.path()).with_idle_timeout(Duration::from_millis(300)),
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
            cache(&upstream.url, dir.path()).with_idle_timeout(Duration::from_millis(500)),
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

    /// 멈춤 한도를 줄인 캐시.
    fn quick(cache: LocalModelCache, idle: Duration) -> LocalModelCache {
        cache.with_idle_timeout(idle)
    }

    #[test]
    fn a_stalled_download_ends_its_thread_and_removes_its_file_and_a_retry_works() {
        let dir = tempfile::tempdir().unwrap();
        let release = Gate::default();
        let held = release.clone();
        let upstream = FakeUpstream::start(move |head, stream| {
            if head.contains("retry") {
                stream
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
                    )
                    .unwrap();
                return;
            }
            // 처음 요청: 일부만 보내고 연결은 연 채로 입을 다문다.
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\n01234",
                )
                .unwrap();
            stream.flush().unwrap();
            held.wait();
        });
        let cache = Arc::new(quick(
            cache(&upstream.url, dir.path()),
            Duration::from_millis(400),
        ));

        let result = fetch(&cache, "model.onnx");

        assert_eq!(
            Err(LocalModelError::Upstream(
                "Model download failed: stalled: no data received".to_string()
            )),
            result
        );
        wait_until("the download thread ends", || cache.live_downloads() == 0);
        assert!(
            files_under(dir.path()).is_empty(),
            "{:?}",
            files_under(dir.path())
        );
        assert!(cache.flights().is_empty());
        let retried = fetch(&cache, "retry.onnx").unwrap();
        assert_eq!(b"ok".to_vec(), std::fs::read(retried.path).unwrap());
        release.open();
    }

    #[test]
    fn a_request_joining_right_after_a_failure_gets_the_upstream_error() {
        let dir = tempfile::tempdir().unwrap();
        // 일부만 보내고 바로 끊는 상위 서버. 받기마다 곧바로 실패한다.
        let upstream = FakeUpstream::start(|_, stream| {
            let _ = stream.write_all(
                b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\n01234",
            );
        });
        let cache = Arc::new(cache(&upstream.url, dir.path()));
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        let joiners: Vec<_> = (0..4)
            .map(|_| {
                let cache = cache.clone();
                std::thread::spawn(move || {
                    let mut internal = Vec::new();
                    while std::time::Instant::now() < deadline {
                        match cache.open(REPO, "main", "model.onnx", None) {
                            Err(LocalModelError::Internal(message)) => internal.push(message),
                            Ok(ModelFile::Downloading(download)) => {
                                let mut body = download.body;
                                let _ = body.read_to_end(&mut Vec::new());
                            }
                            _ => {}
                        }
                    }
                    internal
                })
            })
            .collect();
        let internal: Vec<String> = joiners
            .into_iter()
            .flat_map(|joiner| joiner.join().unwrap())
            .collect();

        let downloads = upstream.requests().len();
        assert!(downloads > 10, "{downloads} downloads");
        assert!(
            internal.is_empty(),
            "{} internal errors in {downloads} downloads: {:?}",
            internal.len(),
            internal.first()
        );
    }

    #[test]
    fn a_disconnect_mid_download_is_not_reported_as_a_timeout() {
        let dir = tempfile::tempdir().unwrap();
        let upstream = FakeUpstream::start(|_, stream| {
            let _ = stream.write_all(
                b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\n01234",
            );
        });
        let cache = cache(&upstream.url, dir.path());

        let message = match fetch(&cache, "model.onnx") {
            Err(LocalModelError::Upstream(message)) => message,
            other => panic!("{other:?}"),
        };

        assert!(
            !message.contains("stalled") && !message.contains("timed out"),
            "{message}"
        );
    }

    #[test]
    fn upstream_404_leaves_no_folder_behind() {
        let dir = tempfile::tempdir().unwrap();
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET);
            then.status(404);
        });
        let cache = cache(&server.base_url(), dir.path());

        let result = fetch(&cache, "onnx/missing.onnx");

        assert!(
            matches!(result, Err(LocalModelError::NotFound(_))),
            "{result:?}"
        );
        assert!(
            !dir.path().join("local-models").exists(),
            "빈 폴더가 남지 않는다"
        );
    }

    #[test]
    fn rejects_overlong_and_windows_unsafe_segments() {
        let dir = tempfile::tempdir().unwrap();
        let cache = cache(UNREACHABLE_UPSTREAM, dir.path());
        let long = "a".repeat(300);
        let deep = vec!["a"; 300].join("/");

        for file in [
            long.as_str(),
            deep.as_str(),
            "x.PART",
            "dir/.download-abc.Part",
            "config.json.",
            "nul",
            "CON",
            "aux.json",
            "onnx/com1.onnx",
            "lpt9",
            "com0.json",
            "onnx/lpt0",
        ] {
            for probe in [None, Some(&ModelFileProbe::default())] {
                let result = cache.open(REPO, "main", file, probe);
                assert!(
                    matches!(result, Err(LocalModelError::InvalidPath(_))),
                    "{}: {result:?}",
                    &file[..file.len().min(40)]
                );
            }
        }
        assert!(!dir.path().join("local-models").exists());
    }

    #[test]
    fn ordinary_names_that_look_like_device_names_are_accepted() {
        let dir = tempfile::tempdir().unwrap();
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET);
            then.status(200).body("x");
        });
        let cache = cache(&server.base_url(), dir.path());

        for file in [
            "console.json",
            "com10.bin",
            "auxiliary.txt",
            "nul_model.onnx",
            "a.part.json",
        ] {
            assert!(fetch(&cache, file).is_ok(), "{file}");
        }
    }

    #[test]
    fn a_range_other_than_the_first_byte_starts_the_download_instead_of_being_relayed() {
        let dir = tempfile::tempdir().unwrap();
        let gate = Gate::default();
        let upstream = split_upstream(&gate, b"56789");
        let cache = Arc::new(cache(&upstream.url, dir.path()));

        for range in ["bytes=7-", "bytes=0-0,5-6", "bytes=-3"] {
            let download = downloading(open_within(
                &cache,
                Some(ModelFileProbe {
                    head: false,
                    range: Some(range.to_string()),
                }),
            ));
            assert_eq!(Some(10), download.length, "{range}");
        }
        gate.open();

        let requests = upstream.requests();
        assert_eq!(1, requests.len(), "{requests:?}");
        assert!(!requests[0].contains("\r\nrange:"), "{}", requests[0]);
        let target = cache_file(dir.path(), "model.onnx");
        wait_until("the file is cached", || target.is_file());
    }

    #[test]
    fn head_does_not_forward_other_ranges() {
        let dir = tempfile::tempdir().unwrap();
        let upstream = FakeUpstream::start(|_, stream| {
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\nConnection: close\r\n\r\n")
                .unwrap();
        });
        let cache = cache(&upstream.url, dir.path());

        let reply = probe(&cache, "model.onnx", true, Some("bytes=0-0,4-9")).unwrap();

        assert!(matches!(reply, ModelFile::Upstream(_)), "{reply:?}");
        let requests = upstream.requests();
        assert!(requests[0].starts_with("head "), "{}", requests[0]);
        assert!(!requests[0].contains("\r\nrange:"), "{}", requests[0]);
    }

    #[test]
    fn an_oversized_probe_reply_is_not_relayed() {
        let dir = tempfile::tempdir().unwrap();
        let upstream = FakeUpstream::start(|_, stream| {
            let _ = stream.write_all(
                b"HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 0-0/10\r\nConnection: close\r\n\r\n",
            );
            let chunk = [b'x'; 64 * 1024];
            for _ in 0..64 {
                if stream.write_all(&chunk).is_err() {
                    return;
                }
            }
        });
        let cache = cache(&upstream.url, dir.path());

        let result = probe(&cache, "model.onnx", false, Some("bytes=0-0"));

        assert!(
            matches!(result, Err(LocalModelError::Upstream(_))),
            "{result:?}"
        );
    }

    #[test]
    fn a_probe_answered_with_the_whole_file_starts_the_download() {
        let dir = tempfile::tempdir().unwrap();
        // 범위를 모르는 상위 서버: `Range`를 무시하고 전체를 보낸다.
        let upstream = FakeUpstream::start(|_, stream| {
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\nConnection: close\r\n\r\n0123456789",
                )
                .unwrap();
        });
        let cache = Arc::new(cache(&upstream.url, dir.path()));

        let opened = open_within(
            &cache,
            Some(ModelFileProbe {
                head: false,
                range: Some("bytes=0-0".to_string()),
            }),
        );

        let body = match opened {
            ModelFile::Downloading(download) => read_rest(download.body).unwrap(),
            ModelFile::Cached(cached) => std::fs::read(cached.path).unwrap(),
            other => panic!("{other:?}"),
        };
        assert_eq!(b"0123456789".to_vec(), body);
        let target = cache_file(dir.path(), "model.onnx");
        wait_until("the file is cached", || target.is_file());
        assert_eq!(2, upstream.requests().len(), "probe, then the download");
    }

    #[test]
    fn the_header_wait_is_longer_than_the_stall_limit_between_body_bytes() {
        let dir = tempfile::tempdir().unwrap();
        // 응답 헤더는 0.8초 뒤에 온다(재전송 뒤 CDN과 TLS 연결 두 번). 본문은 헤더 뒤 바로 온다.
        // `stalled`는 헤더 뒤 일부만 보내고 0.8초 멈춘다.
        let upstream = FakeUpstream::start(|head, stream| {
            if head.contains("stalled") {
                stream
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\nConnection: close\r\n\r\n01234",
                    )
                    .unwrap();
                stream.flush().unwrap();
                std::thread::sleep(Duration::from_millis(800));
                let _ = stream.write_all(b"56789");
                return;
            }
            std::thread::sleep(Duration::from_millis(800));
            let _ = stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok");
        });
        let cache = cache(&upstream.url, dir.path())
            .with_timeouts(Duration::from_millis(2000), Duration::from_millis(400));

        let slow_headers = fetch(&cache, "slow.onnx");
        assert_eq!(
            b"ok".to_vec(),
            std::fs::read(slow_headers.unwrap().path).unwrap(),
            "a header wait within the header limit succeeds"
        );
        assert_eq!(
            Err(LocalModelError::Upstream(
                "Model download failed: stalled: no data received".to_string()
            )),
            fetch(&cache, "stalled.onnx")
        );
    }

    #[test]
    fn the_production_limits_give_the_headers_sixty_seconds_and_a_stall_thirty() {
        assert_eq!(Duration::from_secs(60), HEADER_TIMEOUT);
        assert_eq!(Duration::from_secs(30), STALL_TIMEOUT);
    }

    #[test]
    fn one_http_client_serves_every_probe_and_download_of_a_cache() {
        let dir = tempfile::tempdir().unwrap();
        let upstream = FakeUpstream::start(|head, stream| {
            let reply: &[u8] = if head.starts_with("head ") {
                b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n"
            } else {
                b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok"
            };
            let _ = stream.write_all(reply);
        });
        let cache = cache(&upstream.url, dir.path());
        assert_eq!(0, cache.client_builds(), "built lazily");

        for file in ["a.onnx", "b.onnx"] {
            let ModelFile::Upstream(reply) = probe(&cache, file, true, None).unwrap() else {
                panic!("a probe of an uncached file is relayed");
            };
            assert_eq!(200, reply.status);
            fetch(&cache, file).unwrap();
        }
        fetch(&cache, "c.onnx").unwrap();

        assert_eq!(5, upstream.requests().len());
        assert_eq!(1, cache.client_builds());
        wait_until("download threads end", || cache.live_downloads() == 0);
    }
}
