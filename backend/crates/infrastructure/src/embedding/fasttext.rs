//! fastText `.vec`/`.vec.gz` 모델을 읽어 토큰 벡터의 평균(합 후 정규화)으로 임베딩한다.
//!
//! 모델 수명:
//! - 처음 쓸 때 부른 쪽이 모델을 읽는다(없으면 받는다). 그동안 다른 호출은 그 결과를 기다린다.
//!   부른 쪽이 모델을 받는 것은 프로세스에서 처음 읽을 때뿐이다.
//! - 읽지 못하면 실패를 기억하고, 그 뒤 호출은 기다리지 않고 기억한 실패를 바로 받는다.
//!   다시 시도는 백그라운드 작업자 하나가 한다. 네트워크·디스크 쓰기 실패는 30초부터 두 배씩
//!   늘려 1시간까지 기다렸다가 다시 시도한다. 다시 해도 같은 결과인 실패(다 받은 파일이나 있는
//!   파일의 체크섬 불일치, 차원 불일치, 읽을 수 없거나 벡터가 없는 파일, 빠진 설정)는 모델 파일이
//!   바뀔 때까지(생기거나 크기나 수정 시각이 달라질 때까지) 다시 시도하지 않는다. 설정은
//!   재시작해야 바뀐다.
//! - 읽은 벡터는 `idle_release` 동안 임베딩 호출이 없으면 놓는다. 다음 호출이 디스크에서 다시
//!   읽는다(체크섬을 확인한 뒤로 파일이 그대로면 체크섬은 다시 계산하지 않는다). 그때 파일이
//!   없으면 부른 쪽은 바로 실패를 받고, 받는 일은 백그라운드 작업자가 한다.
//! - 어댑터를 놓으면 작업자와 진행 중인 내려받기가 멈추고 임시 파일도 지워진다.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock, Weak};
use std::time::{Duration, Instant, SystemTime};

use flate2::read::MultiGzDecoder;
use regex::Regex;
use sha2::{Digest, Sha256};

use omypic_application::rulebook::port::EmbeddingPort;
use omypic_application::shared::policy::DataDirProvider;
use omypic_domain::error::{AppError, AppResult};

use super::{is_blank, is_regex_space, normalize, LocalHashEmbeddingAdapter};
use crate::config::RagConfig;

const PROVIDER_NAME: &str = "fasttext";
const DOWNLOAD_STALL_SECONDS: u64 = 60;
const EMBED_CACHE_MAX_ENTRIES: usize = 1000;
/// 다시 시도할 수 있는 실패 뒤 첫 다시 시도까지 기다리는 시간. 실패할 때마다 두 배가 된다.
const RETRY_BACKOFF_INITIAL: Duration = Duration::from_secs(30);
const RETRY_BACKOFF_MAX: Duration = Duration::from_secs(60 * 60);
/// 내려받는 동안 멈춤 요청을 확인하는 간격.
const CANCEL_POLL: Duration = Duration::from_millis(100);
/// 이만큼 줄을 읽을 때마다 멈춤 요청을 확인한다.
const CANCEL_CHECK_LINES: usize = 4096;
const TEMP_PREFIX: &str = ".model-download-";
const TEMP_SUFFIX: &str = ".part";

/// 단어 벡터. 벡터는 한 덩어리(`data`)에 이어 붙여 두어 놓을 때 메모리가 운영체제로 돌아간다.
struct Vectors {
    dim: usize,
    index: HashMap<Box<str>, u32>,
    data: Vec<f32>,
}

impl Vectors {
    fn with_capacity(dim: usize, words: usize) -> Self {
        Self {
            dim,
            index: HashMap::with_capacity(words),
            data: Vec::with_capacity(words.saturating_mul(dim)),
        }
    }

    fn len(&self) -> usize {
        self.index.len()
    }

    fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    fn get(&self, word: &str) -> Option<&[f32]> {
        let start = *self.index.get(word)? as usize * self.dim;
        Some(&self.data[start..start + self.dim])
    }

    /// 같은 단어가 다시 나오면 처음 것을 둔다.
    fn insert(&mut self, word: &str, vector: &[f32]) {
        if self.index.contains_key(word) {
            return;
        }
        let Ok(position) = u32::try_from(self.index.len()) else {
            return;
        };
        self.index.insert(word.into(), position);
        self.data.extend_from_slice(vector);
    }
}

/// 모델 파일을 알아보는 값. 체크섬을 다시 계산할지, 고정 실패를 다시 시도할지 정한다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Fingerprint {
    len: u64,
    modified: Option<SystemTime>,
}

fn fingerprint(path: &Path) -> Option<Fingerprint> {
    let metadata = std::fs::metadata(path).ok()?;
    Some(Fingerprint {
        len: metadata.len(),
        modified: metadata.modified().ok(),
    })
}

enum ModelState {
    /// 아직 읽지 않았거나 쓰지 않아 놓았다. 다음 호출이 읽는다.
    Unloaded,
    /// 호출 하나가 읽는 중이다. 다른 호출은 기다린다.
    Loading,
    Ready {
        vectors: Arc<Vectors>,
        last_used: Instant,
    },
    Failed {
        error: AppError,
        retry: Retry,
        /// 해시 대체를 쓴다는 경고를 이 실패에 대해 이미 남겼다.
        fallback_reported: bool,
    },
}

enum Retry {
    /// 작업자가 `at`에 다시 시도한다. `failures`는 이어서 실패한 횟수.
    At { at: Instant, failures: u32 },
    /// 모델 파일이 이 값과 달라지면 다시 시도한다(`None`은 파일이 없었다는 뜻).
    WhenFileChanges(Option<Fingerprint>),
}

struct Slot {
    model: ModelState,
    worker_running: bool,
    /// 마지막으로 체크섬을 확인한 모델 파일.
    verified: Option<Fingerprint>,
    /// 프로세스에서 처음 읽기를 이미 했다. 그 뒤로 부른 쪽은 모델을 받지 않는다.
    first_load_done: bool,
}

#[derive(Clone, Copy)]
struct Timing {
    /// 본문이 이만큼 한 바이트도 안 오면 내려받기를 실패로 본다.
    stall_timeout: Duration,
    retry_initial: Duration,
    retry_max: Duration,
    /// `None`이면 놓지 않는다.
    idle_release: Option<Duration>,
}

impl Timing {
    fn retry_delay(&self, failures: u32) -> Duration {
        let factor = 1_u32 << failures.saturating_sub(1).min(20);
        self.retry_initial
            .saturating_mul(factor)
            .min(self.retry_max)
    }
}

/// 어댑터를 놓을 때 작업자와 내려받기를 멈춘다.
#[derive(Default)]
struct StopSignal {
    flag: AtomicBool,
    stopped: Mutex<bool>,
    wake: Condvar,
}

impl StopSignal {
    fn stop(&self) {
        self.flag.store(true, Ordering::SeqCst);
        *lock(&self.stopped) = true;
        self.wake.notify_all();
    }

    fn is_stopped(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }

    /// `deadline`까지 기다린다. 그 전에 멈추면 `true`.
    fn wait_until(&self, deadline: Instant) -> bool {
        let mut stopped = lock(&self.stopped);
        loop {
            if *stopped {
                return true;
            }
            let now = Instant::now();
            if now >= deadline {
                return false;
            }
            stopped = self
                .wake
                .wait_timeout(stopped, deadline - now)
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .0;
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

struct LoadFailure {
    error: AppError,
    recovery: Recovery,
}

enum Recovery {
    /// 시간이 지나면 나을 수 있는 실패(네트워크, 디스크 쓰기). 기다렸다가 다시 시도한다.
    Backoff,
    /// 다시 읽을 때 파일이 없었다. 부른 쪽 대신 작업자가 바로 받는다.
    DownloadInBackground,
    /// 다시 해도 같은 결과다. 모델 파일이 바뀌면 다시 시도한다.
    WhenFileChanges,
}

impl LoadFailure {
    fn retryable(error: AppError) -> Self {
        Self {
            error,
            recovery: Recovery::Backoff,
        }
    }

    fn permanent(error: AppError) -> Self {
        Self {
            error,
            recovery: Recovery::WhenFileChanges,
        }
    }
}

pub struct FastTextEmbeddingAdapter {
    inner: Arc<Inner>,
}

struct Inner {
    rag: RagConfig,
    data_dir: Arc<dyn DataDirProvider>,
    hash_fallback: LocalHashEmbeddingAdapter,
    cache: Mutex<EmbedCache>,
    slot: Mutex<Slot>,
    /// 첫 읽기가 끝나면 기다리는 호출을 깨운다.
    loaded: Condvar,
    stop: Arc<StopSignal>,
    timing: Timing,
    /// 체크섬을 확인한 바로 뒤에 한 번 부른다(파일을 그 사이에 바꾸는 시험용).
    #[cfg(test)]
    after_checksum: Mutex<Option<Box<dyn FnOnce() + Send>>>,
    /// 이 횟수만큼 작업자 스레드 띄우기를 실패시킨다(시험용).
    #[cfg(test)]
    failing_spawns: std::sync::atomic::AtomicUsize,
}

impl FastTextEmbeddingAdapter {
    pub fn new(rag: RagConfig, data_dir: Arc<dyn DataDirProvider>) -> Self {
        let hash_fallback = LocalHashEmbeddingAdapter::new(rag.embedding_dim);
        let idle_release =
            Some(Duration::from_millis(rag.model_idle_release_ms)).filter(|d| !d.is_zero());
        Self {
            inner: Arc::new(Inner {
                rag,
                data_dir,
                hash_fallback,
                cache: Mutex::new(EmbedCache::default()),
                slot: Mutex::new(Slot {
                    model: ModelState::Unloaded,
                    worker_running: false,
                    verified: None,
                    first_load_done: false,
                }),
                loaded: Condvar::new(),
                stop: Arc::new(StopSignal::default()),
                timing: Timing {
                    stall_timeout: Duration::from_secs(DOWNLOAD_STALL_SECONDS),
                    retry_initial: RETRY_BACKOFF_INITIAL,
                    retry_max: RETRY_BACKOFF_MAX,
                    idle_release,
                },
                #[cfg(test)]
                after_checksum: Mutex::new(None),
                #[cfg(test)]
                failing_spawns: std::sync::atomic::AtomicUsize::new(0),
            }),
        }
    }

    #[cfg(test)]
    fn timing_mut(&mut self) -> &mut Timing {
        &mut Arc::get_mut(&mut self.inner)
            .expect("configure before first use")
            .timing
    }

    #[cfg(test)]
    fn with_stall_timeout(mut self, stall_timeout: Duration) -> Self {
        self.timing_mut().stall_timeout = stall_timeout;
        self
    }

    #[cfg(test)]
    fn with_retry_backoff(mut self, initial: Duration, max: Duration) -> Self {
        let timing = self.timing_mut();
        timing.retry_initial = initial;
        timing.retry_max = max;
        self
    }

    #[cfg(test)]
    fn with_idle_release(mut self, idle_release: Option<Duration>) -> Self {
        self.timing_mut().idle_release = idle_release;
        self
    }

    #[cfg(test)]
    fn is_loaded(&self) -> bool {
        matches!(lock(&self.inner.slot).model, ModelState::Ready { .. })
    }

    #[cfg(test)]
    fn worker_running(&self) -> bool {
        lock(&self.inner.slot).worker_running
    }

    fn dim(&self) -> usize {
        self.inner.rag.embedding_dim
    }

    #[cfg(test)]
    fn resolve_model_path(&self) -> PathBuf {
        self.inner.resolve_model_path()
    }

    /// `Ok(None)`이면 모델을 쓸 수 없지만 해시 대체가 허용된 상태다.
    fn ensure_vectors(&self) -> AppResult<Option<Arc<Vectors>>> {
        let inner = &self.inner;
        let mut slot = lock(&inner.slot);
        loop {
            match &mut slot.model {
                ModelState::Ready { vectors, last_used } => {
                    *last_used = Instant::now();
                    let vectors = Arc::clone(vectors);
                    // 작업자를 띄우지 못했으면 여기서 다시 띄운다(놓을 때가 되면 놓는다).
                    Inner::ensure_worker(inner, &mut slot);
                    return Ok(Some(vectors));
                }
                ModelState::Loading => {
                    slot = inner
                        .loaded
                        .wait(slot)
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                }
                ModelState::Failed {
                    error,
                    retry,
                    fallback_reported,
                } => {
                    let error = error.clone();
                    let report = !std::mem::replace(fallback_reported, true);
                    if let Retry::WhenFileChanges(seen) = retry {
                        if fingerprint(&inner.resolve_model_path()) != *seen {
                            tracing::info!(
                                "FastText model file changed; loading it again in the background"
                            );
                            *retry = Retry::At {
                                at: Instant::now(),
                                failures: 0,
                            };
                        }
                    }
                    // 다시 시도할 일이 있는데 작업자가 없으면(띄우지 못했으면) 여기서 띄운다.
                    Inner::ensure_worker(inner, &mut slot);
                    drop(slot);
                    return inner.handle_initialization_failure(&error, report);
                }
                ModelState::Unloaded => {
                    slot.model = ModelState::Loading;
                    let verified = slot.verified;
                    let may_download = !std::mem::replace(&mut slot.first_load_done, true);
                    drop(slot);
                    let result = inner.load_catching_panics(verified, may_download);
                    slot = lock(&inner.slot);
                    inner.store_result(&mut slot, result, 0);
                    inner.loaded.notify_all();
                    Inner::ensure_worker(inner, &mut slot);
                }
            }
        }
    }

    fn embed_from_model_or_fallback(&self, text: &str, loaded: &Vectors) -> Vec<f32> {
        let mut sum = vec![0.0_f32; self.dim()];
        let mut hits = 0;
        for token in tokenize(text) {
            if let Some(vector) = loaded.get(token.as_str()) {
                for (target, source) in sum.iter_mut().zip(vector) {
                    *target += *source;
                }
                hits += 1;
            }
        }
        if hits > 0 {
            normalize(&mut sum);
            return sum;
        }
        if self.inner.rag.allow_hash_fallback {
            return self.inner.hash_fallback.embed_text(text);
        }
        sum
    }
}

impl Drop for FastTextEmbeddingAdapter {
    fn drop(&mut self) {
        self.inner.stop.stop();
    }
}

impl Inner {
    /// `report`이면 해시 대체를 쓴다고 남긴다(실패 하나에 한 번).
    fn handle_initialization_failure(
        &self,
        failure: &AppError,
        report: bool,
    ) -> AppResult<Option<Arc<Vectors>>> {
        if self.rag.allow_hash_fallback {
            if report {
                tracing::warn!("FastText model unavailable; using hash fallback. reason={failure}");
            }
            return Ok(None);
        }
        Err(failure.clone())
    }

    fn store_result(
        &self,
        slot: &mut Slot,
        result: Result<(Vectors, Option<Fingerprint>), LoadFailure>,
        failures_before: u32,
    ) {
        slot.model = match result {
            Ok((vectors, verified)) => {
                slot.verified = verified;
                ModelState::Ready {
                    vectors: Arc::new(vectors),
                    last_used: Instant::now(),
                }
            }
            Err(LoadFailure {
                error,
                recovery: Recovery::DownloadInBackground,
            }) => {
                tracing::warn!("{error}");
                ModelState::Failed {
                    error,
                    retry: Retry::At {
                        at: Instant::now(),
                        failures: 0,
                    },
                    fallback_reported: false,
                }
            }
            Err(LoadFailure {
                error,
                recovery: Recovery::Backoff,
            }) => {
                let failures = failures_before.saturating_add(1);
                let delay = self.timing.retry_delay(failures);
                tracing::warn!(
                    "FastText model could not be loaded; retrying in the background in {}s. reason={error}",
                    delay.as_secs()
                );
                ModelState::Failed {
                    error,
                    retry: Retry::At {
                        at: Instant::now() + delay,
                        failures,
                    },
                    fallback_reported: false,
                }
            }
            Err(LoadFailure {
                error,
                recovery: Recovery::WhenFileChanges,
            }) => {
                tracing::warn!(
                    "FastText model could not be loaded; not retrying until the model file changes \
                     or OMyPIc restarts. reason={error}"
                );
                ModelState::Failed {
                    error,
                    retry: Retry::WhenFileChanges(fingerprint(&self.resolve_model_path())),
                    fallback_reported: false,
                }
            }
        };
    }

    fn needs_worker(&self, model: &ModelState) -> bool {
        match model {
            ModelState::Ready { .. } => self.timing.idle_release.is_some(),
            ModelState::Failed {
                retry: Retry::At { .. },
                ..
            } => true,
            _ => false,
        }
    }

    /// 다시 시도하거나 놓을 일이 있으면 작업자를 하나만 띄운다. 작업자는 `Weak`만 쥐므로
    /// 어댑터의 수명을 늘리지 않는다.
    fn ensure_worker(this: &Arc<Self>, slot: &mut Slot) {
        if slot.worker_running || this.stop.is_stopped() || !this.needs_worker(&slot.model) {
            return;
        }
        let weak = Arc::downgrade(this);
        match this.spawn_worker(weak) {
            Ok(_) => slot.worker_running = true,
            Err(error) => tracing::warn!("Failed to start the FastText model worker: {error}"),
        }
    }

    fn spawn_worker(&self, weak: Weak<Self>) -> std::io::Result<std::thread::JoinHandle<()>> {
        #[cfg(test)]
        if self
            .failing_spawns
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
            .is_ok()
        {
            return Err(std::io::Error::other("injected spawn failure"));
        }
        std::thread::Builder::new()
            .name("fasttext-model".to_string())
            .spawn(move || run_worker(weak))
    }

    fn load_catching_panics(
        &self,
        verified: Option<Fingerprint>,
        may_download: bool,
    ) -> Result<(Vectors, Option<Fingerprint>), LoadFailure> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.load_model(verified, may_download)
        }))
        .unwrap_or_else(|_| {
            Err(LoadFailure::permanent(AppError::Internal(
                "FastText model loading panicked".to_string(),
            )))
        })
    }

    /// 읽은 벡터와, 체크섬을 확인한 뒤로 파일이 바뀌지 않았으면 그 파일의 지문을 돌려준다.
    fn load_model(
        &self,
        verified: Option<Fingerprint>,
        may_download: bool,
    ) -> Result<(Vectors, Option<Fingerprint>), LoadFailure> {
        let started = Instant::now();
        let (model_path, checked) = self.ensure_model_file(verified, may_download)?;
        let checksum_time = started.elapsed();
        let vectors = self
            .load_vectors(&model_path)
            .map_err(LoadFailure::permanent)?;
        let current = fingerprint(&model_path).ok_or_else(|| {
            LoadFailure::retryable(AppError::InvalidState(format!(
                "FastText model disappeared while loading: {}",
                model_path.display()
            )))
        })?;
        tracing::info!(
            "RAG fasttext model loaded path={} vocab={} checksum_ms={} total_ms={}",
            model_path.display(),
            vectors.len(),
            checksum_time.as_millis(),
            started.elapsed().as_millis()
        );
        // 체크섬 확인과 읽기 사이에 바뀐 파일은 확인한 파일이 아니다. 다음에 다시 확인한다.
        let verified = checked.filter(|checked| *checked == current);
        if verified.is_none() {
            tracing::warn!(
                "FastText model file changed while it was read; checking it again on the next load"
            );
        }
        Ok((vectors, verified))
    }

    /// 모델 파일의 경로와, 체크섬을 확인한(또는 받은) 그 파일의 지문을 돌려준다.
    /// `may_download`가 아니면 없는 파일을 받지 않고 작업자에게 넘기는 실패를 돌려준다.
    fn ensure_model_file(
        &self,
        verified: Option<Fingerprint>,
        may_download: bool,
    ) -> Result<(PathBuf, Option<Fingerprint>), LoadFailure> {
        let model_path = self.resolve_model_path();
        if let Some(parent) = model_path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| {
                LoadFailure::retryable(AppError::InvalidState(format!(
                    "Failed to prepare model directory: {}: {err}",
                    model_path.display()
                )))
            })?;
        }
        if model_path.exists() {
            // 체크섬보다 먼저 잰다. 그 뒤에 바뀐 파일은 읽은 뒤의 지문과 달라진다.
            let before = fingerprint(&model_path);
            if before.is_none() || before != verified {
                self.verify_model_sha256(&model_path)
                    .map_err(LoadFailure::permanent)?;
            }
            #[cfg(test)]
            if let Some(hook) = lock(&self.after_checksum).take() {
                hook();
            }
            return Ok((model_path, before));
        }
        if !may_download {
            self.download_settings().map_err(LoadFailure::permanent)?;
            return Err(LoadFailure {
                error: AppError::InvalidState(format!(
                    "FastText model file is missing: {}; downloading it in the background",
                    model_path.display()
                )),
                recovery: Recovery::DownloadInBackground,
            });
        }
        self.download_model_with_retry(&model_path)?;
        let downloaded = fingerprint(&model_path);
        Ok((model_path, downloaded))
    }

    fn resolve_model_path(&self) -> PathBuf {
        let configured = self.rag.model_path.trim();
        if !configured.is_empty() {
            return PathBuf::from(java_trim(&self.rag.model_path));
        }
        let version = match java_trim(&self.rag.model_version) {
            "" => "fasttext.vec.gz",
            version => version,
        };
        Path::new(&self.data_dir.get_data_dir())
            .join("models")
            .join(version)
    }

    fn expected_sha256(&self) -> AppResult<&str> {
        let expected = java_trim(&self.rag.model_sha256);
        if expected.is_empty() {
            return Err(AppError::InvalidState(
                "omypic.rag.model-sha256 is required for fasttext provider".to_string(),
            ));
        }
        Ok(expected)
    }

    fn verify_model_sha256(&self, model_path: &Path) -> AppResult<()> {
        let expected = self.expected_sha256()?;
        let actual = sha256_file(model_path, &self.stop)?;
        check_sha256(expected, &actual, model_path)
    }

    /// 받을 주소와 기대하는 체크섬. 받기 전에 확인한다(체크섬 없이 받은 파일은 어차피 쓸 수 없다).
    fn download_settings(&self) -> AppResult<(&str, &str)> {
        let url = java_trim(&self.rag.download_url);
        if url.is_empty() {
            return Err(AppError::InvalidState(
                "FastText model is missing and omypic.rag.download-url is not configured"
                    .to_string(),
            ));
        }
        Ok((url, self.expected_sha256()?))
    }

    fn download_model_with_retry(&self, model_path: &Path) -> Result<(), LoadFailure> {
        let (url, expected) = self.download_settings().map_err(LoadFailure::permanent)?;
        let attempts = self.rag.download_retry_max.saturating_add(1).max(1);
        let mut last_failure = String::new();
        for attempt in 1..=attempts {
            match self.download_model(model_path, url, expected) {
                Ok(()) => return Ok(()),
                Err(DownloadError::Fatal(err)) => return Err(LoadFailure::permanent(err)),
                Err(DownloadError::Transfer(message)) => {
                    last_failure = message;
                    if attempt < attempts && self.stop.wait_until(Instant::now() + backoff(attempt))
                    {
                        break;
                    }
                }
            }
            if self.stop.is_stopped() {
                break;
            }
        }
        Err(LoadFailure::retryable(AppError::InvalidState(format!(
            "Failed to download FastText model after retries: {last_failure}"
        ))))
    }

    /// 같은 디렉터리의 임시 파일에 받고, 체크섬이 맞을 때만 이름을 바꿔 넣는다.
    ///
    /// `download-timeout-seconds`는 연결과 응답 헤더를 기다리는 시간만 제한한다(Java와 같다).
    /// 본문은 느려도 계속 오는 한 끊지 않고, `stall_timeout` 동안 한 바이트도 안 오면 실패한다.
    /// 받는 일은 전용 스레드의 단일 스레드 런타임에서 하고, 제한 시간이나 멈춤 요청이 오면
    /// 그 자리에서 연결과 임시 파일을 놓는다(블로킹 클라이언트는 본문 읽기마다 헤더 제한 시간을
    /// 써서 멈춘 연결을 그만큼 오래 붙잡았다).
    fn download_model(
        &self,
        model_path: &Path,
        url: &str,
        expected: &str,
    ) -> Result<(), DownloadError> {
        let job = DownloadJob {
            url: url.to_string(),
            model_path: model_path.to_path_buf(),
            expected: expected.to_string(),
            header_timeout: Duration::from_secs(self.rag.download_timeout_seconds.max(1)),
            stall_timeout: self.timing.stall_timeout,
            stop: Arc::clone(&self.stop),
        };
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("fasttext-download".to_string())
            .spawn(move || {
                let _ = sender.send(job.run());
            })
            .map_err(|err| {
                DownloadError::Transfer(format!("Model download failed to start: {err}"))
            })?;
        receiver.recv().unwrap_or_else(|_| {
            Err(DownloadError::Transfer(
                "Model download failed: the download thread ended unexpectedly".to_string(),
            ))
        })
    }

    fn load_vectors(&self, model_path: &Path) -> AppResult<Vectors> {
        let load_error = |err: std::io::Error| {
            AppError::InvalidState(format!(
                "Failed to load FastText vectors from {}: {err}",
                model_path.display()
            ))
        };
        let dim = self.rag.embedding_dim;
        let max_vocab = self.rag.model_max_vocab.max(1);
        let mut lines = LineReader::new(open_reader(model_path).map_err(load_error)?);

        let Some(first_line) = lines.next_line().map_err(load_error)? else {
            return Err(AppError::InvalidState(format!(
                "FastText model is empty: {}",
                model_path.display()
            )));
        };
        let header = parse_header(&first_line)?;
        self.validate_dimension(header.map(|(_, dim)| dim))?;
        let words = header
            .and_then(|(count, _)| usize::try_from(count).ok())
            .map_or(max_vocab, |count| count.min(max_vocab));
        let mut loaded = Vectors::with_capacity(dim, words);
        let mut vector = Vec::with_capacity(dim);
        if header.is_none() {
            self.parse_vector_line(&first_line, &mut vector, &mut loaded);
        }
        let mut read = 0_usize;
        while loaded.len() < max_vocab {
            let Some(line) = lines.next_line().map_err(load_error)? else {
                break;
            };
            self.parse_vector_line(&line, &mut vector, &mut loaded);
            read += 1;
            if read.is_multiple_of(CANCEL_CHECK_LINES) && self.stop.is_stopped() {
                return Err(AppError::InvalidState(
                    "FastText model loading was cancelled".to_string(),
                ));
            }
        }
        if loaded.is_empty() {
            return Err(AppError::InvalidState(format!(
                "No vectors were loaded from model: {}",
                model_path.display()
            )));
        }
        loaded.data.shrink_to_fit();
        Ok(loaded)
    }

    fn validate_dimension(&self, header_dim: Option<i32>) -> AppResult<()> {
        let dim = self.rag.embedding_dim;
        match header_dim {
            Some(actual) if i64::from(actual) != dim as i64 => Err(AppError::InvalidState(
                format!("Embedding dimension mismatch. expected={dim} actual={actual}"),
            )),
            _ => Ok(()),
        }
    }

    fn parse_vector_line(&self, line: &str, vector: &mut Vec<f32>, loaded: &mut Vectors) {
        let dim = self.rag.embedding_dim;
        let trimmed = java_trim(line);
        if trimmed.is_empty() {
            return;
        }
        let mut parts = trimmed
            .split(is_regex_space)
            .filter(|part| !part.is_empty());
        let Some(word) = parts.next() else {
            return;
        };
        vector.clear();
        vector.extend(parts.take(dim).map(parse_java_float));
        if vector.len() < dim {
            return;
        }
        normalize(vector);
        loaded.insert(word, vector);
    }
}

/// 다시 시도할 실패가 있거나 놓을 벡터가 있는 동안만 돈다. 어댑터가 사라지거나 멈추면 끝난다.
fn run_worker(weak: Weak<Inner>) {
    enum Step {
        Exit,
        Wait(Instant),
        Load(u32),
        Release,
    }
    loop {
        let Some(inner) = weak.upgrade() else {
            return;
        };
        if inner.stop.is_stopped() {
            return;
        }
        let mut slot = lock(&inner.slot);
        let now = Instant::now();
        let step = match &slot.model {
            ModelState::Ready { last_used, .. } => match inner.timing.idle_release {
                Some(idle) if now >= *last_used + idle => Step::Release,
                Some(idle) => Step::Wait(*last_used + idle),
                None => Step::Exit,
            },
            ModelState::Failed {
                retry: Retry::At { at, failures },
                ..
            } => {
                if now >= *at {
                    Step::Load(*failures)
                } else {
                    Step::Wait(*at)
                }
            }
            _ => Step::Exit,
        };
        match step {
            Step::Exit => {
                slot.worker_running = false;
                return;
            }
            Step::Release => {
                let released = std::mem::replace(&mut slot.model, ModelState::Unloaded);
                drop(slot);
                // 벡터를 쓰는 중인 호출이 있으면 그 호출이 끝날 때 놓인다.
                drop(released);
                tracing::info!(
                    "Released the FastText model after {}s without embedding requests",
                    inner.timing.idle_release.unwrap_or_default().as_secs()
                );
            }
            Step::Wait(deadline) => {
                drop(slot);
                let stop = Arc::clone(&inner.stop);
                drop(inner);
                if stop.wait_until(deadline) {
                    return;
                }
            }
            Step::Load(failures) => {
                let verified = slot.verified;
                drop(slot);
                let result = inner.load_catching_panics(verified, true);
                if inner.stop.is_stopped() {
                    return;
                }
                let mut slot = lock(&inner.slot);
                if matches!(slot.model, ModelState::Failed { .. }) {
                    inner.store_result(&mut slot, result, failures);
                }
                inner.loaded.notify_all();
            }
        }
    }
}

impl EmbeddingPort for FastTextEmbeddingAdapter {
    fn embed(&self, text: &str) -> AppResult<Vec<f32>> {
        if is_blank(text) {
            return Ok(vec![0.0; self.dim()]);
        }
        let cache_key = java_trim(text).to_lowercase();
        if let Some(cached) = self.lock_cache().get(&cache_key) {
            return Ok(cached);
        }
        let computed = match self.ensure_vectors()? {
            // 모델을 쓸 수 있게 되면 진짜 벡터를 돌려주도록 대체 벡터는 캐시하지 않는다.
            None => return Ok(self.inner.hash_fallback.embed_text(text)),
            Some(loaded) => self.embed_from_model_or_fallback(text, &loaded),
        };
        self.lock_cache().put(cache_key, computed.clone());
        Ok(computed)
    }

    fn dimension(&self) -> usize {
        self.dim()
    }

    fn provider(&self) -> String {
        PROVIDER_NAME.to_string()
    }

    fn model_version(&self) -> String {
        match java_trim(&self.inner.rag.model_version) {
            "" => "unknown-model".to_string(),
            version => version.to_string(),
        }
    }
}

impl FastTextEmbeddingAdapter {
    fn lock_cache(&self) -> MutexGuard<'_, EmbedCache> {
        lock(&self.inner.cache)
    }
}

struct DownloadJob {
    url: String,
    model_path: PathBuf,
    expected: String,
    /// 연결과 응답 헤더를 기다리는 최대 시간.
    header_timeout: Duration,
    /// 본문 조각 하나를 기다리는 최대 시간.
    stall_timeout: Duration,
    stop: Arc<StopSignal>,
}

impl DownloadJob {
    fn run(self) -> Result<(), DownloadError> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|err| DownloadError::Transfer(format!("Model download failed: {err}")))?;
        let stop = Arc::clone(&self.stop);
        let cancelled = async move {
            while !stop.is_stopped() {
                tokio::time::sleep(CANCEL_POLL).await;
            }
        };
        // 멈추면 받던 future를 버린다. 연결과 임시 파일(`NamedTempFile`)이 그때 함께 놓인다.
        runtime.block_on(async {
            tokio::select! {
                biased;
                () = cancelled => Err(DownloadError::Transfer(
                    "Model download cancelled".to_string(),
                )),
                result = self.fetch() => result,
            }
        })
    }

    async fn fetch(&self) -> Result<(), DownloadError> {
        let transfer = |err: &dyn std::fmt::Display| {
            DownloadError::Transfer(format!("Model download failed: {err}"))
        };
        let client = crate::http_client::build_async_client(|| {
            reqwest::Client::builder()
                .connect_timeout(self.header_timeout)
                .tcp_keepalive(Some(Duration::from_secs(30)))
        })
        .map_err(|err| transfer(&err))?;
        let mut response = tokio::time::timeout(self.header_timeout, client.get(&self.url).send())
            .await
            .map_err(|_| transfer(&"timed out waiting for response headers"))?
            .map_err(|err| transfer(&err))?;
        if !response.status().is_success() {
            return Err(DownloadError::Transfer(format!(
                "Model download failed with status {}",
                response.status().as_u16()
            )));
        }

        let dir = self.model_path.parent().unwrap_or_else(|| Path::new("."));
        remove_stale_temp_files(dir, self.stall_timeout.saturating_mul(2));
        let mut temp = tempfile::Builder::new()
            .prefix(TEMP_PREFIX)
            .suffix(TEMP_SUFFIX)
            .tempfile_in(dir)
            .map_err(|err| transfer(&err))?;
        let mut hasher = Sha256::new();
        loop {
            let chunk = tokio::time::timeout(self.stall_timeout, response.chunk())
                .await
                .map_err(|_| transfer(&"stalled: no data received"))?
                .map_err(|err| transfer(&err))?;
            let Some(chunk) = chunk else {
                break;
            };
            hasher.update(&chunk);
            temp.write_all(&chunk).map_err(|err| transfer(&err))?;
        }
        temp.flush().map_err(|err| transfer(&err))?;

        check_sha256(
            &self.expected,
            &to_hex(&hasher.finalize()),
            &self.model_path,
        )
        .map_err(DownloadError::Fatal)?;
        temp.persist(&self.model_path)
            .map_err(|err| transfer(&err.error))?;
        Ok(())
    }
}

/// 끝나지 않은 내려받기의 임시 파일(프로세스가 받는 중에 죽었다) 가운데 `older_than`보다
/// 오래 쓰이지 않은 것을 지운다. 받는 중인 파일은 적어도 `stall_timeout`마다 쓰인다.
fn remove_stale_temp_files(dir: &Path, older_than: Duration) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with(TEMP_PREFIX) || !name.ends_with(TEMP_SUFFIX) {
            continue;
        }
        let stale = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .is_some_and(|age| age >= older_than);
        if stale {
            match std::fs::remove_file(entry.path()) {
                Ok(()) => tracing::info!("Removed unfinished model download {name}"),
                Err(error) => {
                    tracing::warn!("Failed to remove unfinished download {name}: {error}")
                }
            }
        }
    }
}

enum DownloadError {
    /// 네트워크나 파일 쓰기 실패. 다시 시도한다.
    Transfer(String),
    /// 받은 파일이 틀렸다. 다시 시도해도 같은 결과라 바로 실패한다.
    Fatal(AppError),
}

/// 접근 순서(LRU)를 지키는 최대 1000개짜리 캐시. Java `LinkedHashMap(accessOrder)`와 같다.
#[derive(Default)]
struct EmbedCache {
    entries: HashMap<String, (Vec<f32>, u64)>,
    tick: u64,
}

impl EmbedCache {
    fn get(&mut self, key: &str) -> Option<Vec<f32>> {
        self.tick += 1;
        let tick = self.tick;
        self.entries.get_mut(key).map(|entry| {
            entry.1 = tick;
            entry.0.clone()
        })
    }

    fn put(&mut self, key: String, value: Vec<f32>) {
        self.tick += 1;
        self.entries.insert(key, (value, self.tick));
        if self.entries.len() > EMBED_CACHE_MAX_ENTRIES {
            let eldest = self
                .entries
                .iter()
                .min_by_key(|(_, (_, tick))| *tick)
                .map(|(key, _)| key.clone());
            if let Some(eldest) = eldest {
                self.entries.remove(&eldest);
            }
        }
    }
}

/// Java 코드의 토큰화: 소문자로 바꾸고 글자(`\p{L}`)와 십진 숫자(`\p{Nd}`)가 아닌 것으로 나눈다.
fn tokenize(text: &str) -> Vec<String> {
    static NON_WORD: OnceLock<Regex> = OnceLock::new();
    let regex = NON_WORD.get_or_init(|| Regex::new(r"[^\p{L}\p{Nd}]+").expect("valid regex"));
    regex
        .split(&text.to_lowercase())
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .collect()
}

fn java_trim(text: &str) -> &str {
    text.trim_matches(|c: char| c <= ' ')
}

fn backoff(attempt: u32) -> Duration {
    let shift = attempt.saturating_sub(1).min(20);
    Duration::from_millis((500_u64 << shift).min(5000))
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn sha256_file(path: &Path, stop: &StopSignal) -> AppResult<String> {
    let read_error = |err: std::io::Error| {
        AppError::InvalidState(format!(
            "Failed to read model file for checksum: {}: {err}",
            path.display()
        ))
    };
    let mut file = File::open(path).map_err(read_error)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 256 * 1024];
    loop {
        if stop.is_stopped() {
            return Err(AppError::InvalidState(
                "FastText model checksum was cancelled".to_string(),
            ));
        }
        let read = file.read(&mut buffer).map_err(read_error)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(to_hex(&hasher.finalize()))
}

fn check_sha256(expected: &str, actual: &str, model_path: &Path) -> AppResult<()> {
    if expected.eq_ignore_ascii_case(actual) {
        Ok(())
    } else {
        Err(AppError::InvalidState(format!(
            "FastText model sha256 mismatch for {}",
            model_path.display()
        )))
    }
}

fn open_reader(model_path: &Path) -> std::io::Result<Box<dyn BufRead>> {
    let file = File::open(model_path)?;
    let is_gz = model_path
        .file_name()
        .is_some_and(|name| name.to_string_lossy().ends_with(".gz"));
    if is_gz {
        Ok(Box::new(BufReader::new(MultiGzDecoder::new(file))))
    } else {
        Ok(Box::new(BufReader::new(file)))
    }
}

/// Java `BufferedReader.readLine`처럼 `\n`, `\r`, `\r\n`을 줄 끝으로 보고, 잘못된 UTF-8은 U+FFFD로 바꾼다.
struct LineReader {
    reader: Box<dyn BufRead>,
    pending: std::collections::VecDeque<String>,
}

impl LineReader {
    fn new(reader: Box<dyn BufRead>) -> Self {
        Self {
            reader,
            pending: std::collections::VecDeque::new(),
        }
    }

    fn next_line(&mut self) -> std::io::Result<Option<String>> {
        if let Some(line) = self.pending.pop_front() {
            return Ok(Some(line));
        }
        let mut raw = Vec::new();
        if self.reader.read_until(b'\n', &mut raw)? == 0 {
            return Ok(None);
        }
        if raw.last() == Some(&b'\n') {
            raw.pop();
        }
        if raw.last() == Some(&b'\r') {
            raw.pop();
        }
        let text = String::from_utf8_lossy(&raw).into_owned();
        let mut pieces = text.split('\r').map(str::to_string);
        let first = pieces.next().unwrap_or_default();
        self.pending.extend(pieces);
        Ok(Some(first))
    }
}

/// 첫 줄이 `<정수> <정수>` 두 토큰이면 헤더이고 `(단어 수, 차원)`이다. 단어 수가 너무 크면
/// 0으로 본다(미리 잡을 크기에만 쓴다).
fn parse_header(line: &str) -> AppResult<Option<(u64, i32)>> {
    let parts: Vec<&str> = java_trim(line)
        .split(is_regex_space)
        .filter(|part| !part.is_empty())
        .collect();
    if parts.len() != 2 || !parts.iter().all(|part| is_integer(part)) {
        return Ok(None);
    }
    let dim = parts[1]
        .parse::<i32>()
        .map_err(|_| AppError::InvalidArgument(format!("For input string: \"{}\"", parts[1])))?;
    Ok(Some((parts[0].parse().unwrap_or(0), dim)))
}

fn is_integer(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|c| c.is_ascii_digit())
}

/// Java `Float.parseFloat`을 따르되 실패하면 0을 돌려준다(`NaN`, `Infinity`, 끝의 `f`/`d` 포함).
fn parse_java_float(value: &str) -> f32 {
    let text = java_trim(value);
    let (negative, body) = match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    let apply_sign = |v: f32| if negative { -v } else { v };
    match body {
        "NaN" => return f32::NAN,
        "Infinity" => return apply_sign(f32::INFINITY),
        _ => {}
    }
    let digits = match body.strip_suffix(['f', 'F', 'd', 'D']) {
        Some(stripped) => stripped,
        None => body,
    };
    if digits.is_empty()
        || !digits
            .chars()
            .all(|c| c.is_ascii_digit() || ".eE+-".contains(c))
    {
        return 0.0;
    }
    digits.parse::<f32>().map(apply_sign).unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::FixedDataDir;
    use httpmock::prelude::*;

    fn sha256_hex(bytes: &[u8]) -> String {
        to_hex(&Sha256::digest(bytes))
    }

    fn rag(model_path: &Path, sha256: &str) -> RagConfig {
        RagConfig {
            provider: "fasttext".to_string(),
            embedding_dim: 3,
            max_context_chunks: 3,
            model_path: model_path.to_string_lossy().into_owned(),
            model_version: model_path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            model_sha256: sha256.to_string(),
            download_url: String::new(),
            download_timeout_seconds: 3,
            download_retry_max: 0,
            allow_hash_fallback: false,
            model_max_vocab: 100,
            model_idle_release_ms: 0,
        }
    }

    /// `check`가 참이 될 때까지 기다린다(최대 `limit`).
    fn eventually(limit: Duration, mut check: impl FnMut() -> bool) -> bool {
        let deadline = Instant::now() + limit;
        loop {
            if check() {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn adapter(rag: RagConfig, dir: &Path) -> FastTextEmbeddingAdapter {
        FastTextEmbeddingAdapter::new(rag, Arc::new(FixedDataDir(dir.to_string_lossy().into())))
    }

    fn write_model(dir: &Path, name: &str, content: &str) -> (PathBuf, String) {
        let path = dir.join(name);
        std::fs::write(&path, content).unwrap();
        (path, sha256_hex(content.as_bytes()))
    }

    const MODEL: &str = "3 3\nhello 1 0 0\nworld 0 1 0\nalpha 0 0 1\n";

    fn l2_norm(vector: &[f32]) -> f64 {
        vector
            .iter()
            .map(|v| f64::from(*v) * f64::from(*v))
            .sum::<f64>()
            .sqrt()
    }

    fn dir_is_empty(dir: &Path) -> bool {
        std::fs::read_dir(dir).unwrap().next().is_none()
    }

    #[test]
    fn embed_loads_local_model_and_normalizes_vector() {
        let dir = tempfile::tempdir().unwrap();
        let (path, sha) = write_model(dir.path(), "ko.vec", MODEL);
        let adapter = adapter(rag(&path, &sha), dir.path());

        let vector = adapter.embed("hello world").unwrap();

        assert_eq!(3, vector.len());
        assert!(vector[0] > 0.0);
        assert!(vector[1] > 0.0);
        assert!((l2_norm(&vector) - 1.0).abs() < 1e-6);
        // [1, 1, 0] / sqrt(2)를 Java와 같은 순서(double 누적, float 변환)로 계산한 값이다.
        assert_eq!(vec![0.707_106_77_f32, 0.707_106_77_f32, 0.0], vector);
    }

    #[test]
    fn embed_downloads_model_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        let body = "2 3\nhello 1 0 0\nworld 0 1 0\n";
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).path("/ko.vec");
            then.status(200).body(body);
        });
        let path = dir.path().join("downloaded.vec");
        let mut config = rag(&path, &sha256_hex(body.as_bytes()));
        config.download_url = server.url("/ko.vec");
        let adapter = adapter(config, dir.path());

        let vector = adapter.embed("hello").unwrap();

        mock.assert();
        assert!(path.exists());
        assert_eq!(vec![1.0, 0.0, 0.0], vector);
        assert_eq!(1, std::fs::read_dir(dir.path()).unwrap().count());
    }

    #[test]
    fn embed_uses_hash_fallback_when_model_init_fails_and_fallback_enabled() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = rag(&dir.path().join("missing.vec"), "deadbeef");
        config.allow_hash_fallback = true;
        let adapter = adapter(config, dir.path());

        let vector = adapter.embed("fallback token").unwrap();

        assert_eq!(3, vector.len());
        assert!(l2_norm(&vector) > 0.0);
        assert_eq!(
            LocalHashEmbeddingAdapter::new(3).embed_text("fallback token"),
            vector
        );
    }

    #[test]
    fn init_failure_without_fallback_is_reported_every_time() {
        let dir = tempfile::tempdir().unwrap();
        let (path, _) = write_model(dir.path(), "ko.vec", MODEL);
        let adapter = adapter(rag(&path, "deadbeef"), dir.path());

        let expected = AppError::InvalidState(format!(
            "FastText model sha256 mismatch for {}",
            path.display()
        ));
        assert_eq!(expected, adapter.embed("hello").unwrap_err());
        assert_eq!(expected, adapter.embed("world").unwrap_err());
    }

    #[test]
    fn blank_text_returns_zero_vector_without_loading_the_model() {
        let dir = tempfile::tempdir().unwrap();
        let adapter = adapter(rag(&dir.path().join("missing.vec"), "x"), dir.path());
        assert_eq!(vec![0.0; 3], adapter.embed("  \t").unwrap());
        assert_eq!(vec![0.0; 3], adapter.embed("").unwrap());
    }

    #[test]
    fn gz_model_is_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ko.vec.gz");
        let mut encoder = flate2::write::GzEncoder::new(
            File::create(&path).unwrap(),
            flate2::Compression::fast(),
        );
        encoder.write_all(MODEL.as_bytes()).unwrap();
        encoder.finish().unwrap();
        let sha = sha256_hex(&std::fs::read(&path).unwrap());
        let adapter = adapter(rag(&path, &sha), dir.path());

        assert_eq!(vec![0.0, 0.0, 1.0], adapter.embed("alpha").unwrap());
    }

    #[test]
    fn model_max_vocab_limits_loaded_words() {
        let dir = tempfile::tempdir().unwrap();
        let (path, sha) = write_model(dir.path(), "ko.vec", MODEL);
        let mut config = rag(&path, &sha);
        config.model_max_vocab = 2;
        let adapter = adapter(config, dir.path());

        assert_eq!(vec![0.0, 1.0, 0.0], adapter.embed("world").unwrap());
        // 셋째 단어는 읽지 않았으므로 모르는 토큰이다(대체 없음 -> 영벡터).
        assert_eq!(vec![0.0; 3], adapter.embed("alpha").unwrap());
    }

    #[test]
    fn unknown_tokens_are_ignored_and_tokenization_splits_on_punctuation() {
        let dir = tempfile::tempdir().unwrap();
        let (path, sha) = write_model(dir.path(), "ko.vec", "3 3\nhello 1 0 0\n안녕 0 3 4\n");
        let adapter = adapter(rag(&path, &sha), dir.path());

        let plain = adapter.embed("hello").unwrap();
        assert_eq!(plain, adapter.embed("Hello, UNKNOWN!").unwrap());
        assert_eq!(plain, adapter.embed("hello-hello").unwrap()[..]);
        assert_eq!(vec![0.0, 0.6, 0.8], adapter.embed("안녕!").unwrap());
    }

    #[test]
    fn no_known_token_returns_zero_vector_or_hash_vector_by_policy() {
        let dir = tempfile::tempdir().unwrap();
        let (path, sha) = write_model(dir.path(), "ko.vec", MODEL);

        let strict = adapter(rag(&path, &sha), dir.path());
        assert_eq!(vec![0.0; 3], strict.embed("zzz").unwrap());

        let mut config = rag(&path, &sha);
        config.allow_hash_fallback = true;
        let lenient = adapter(config, dir.path());
        assert_eq!(
            LocalHashEmbeddingAdapter::new(3).embed_text("zzz"),
            lenient.embed("zzz").unwrap()
        );
    }

    #[test]
    fn header_dimension_mismatch_fails_initialization() {
        let dir = tempfile::tempdir().unwrap();
        let (path, sha) = write_model(dir.path(), "ko.vec", "2 4\nhello 1 0 0 0\n");
        let adapter = adapter(rag(&path, &sha), dir.path());

        assert_eq!(
            AppError::InvalidState("Embedding dimension mismatch. expected=3 actual=4".to_string()),
            adapter.embed("hello").unwrap_err()
        );
    }

    #[test]
    fn model_without_header_uses_first_line_as_vector() {
        let dir = tempfile::tempdir().unwrap();
        let (path, sha) = write_model(dir.path(), "ko.vec", "hello 1 0 0\r\nworld 0 1 0\r\n");
        let adapter = adapter(rag(&path, &sha), dir.path());

        assert_eq!(vec![1.0, 0.0, 0.0], adapter.embed("hello").unwrap());
        assert_eq!(vec![0.0, 1.0, 0.0], adapter.embed("world").unwrap());
    }

    #[test]
    fn empty_model_and_vectorless_model_fail() {
        let dir = tempfile::tempdir().unwrap();
        let (empty, sha) = write_model(dir.path(), "empty.vec", "");
        assert_eq!(
            AppError::InvalidState(format!("FastText model is empty: {}", empty.display())),
            adapter(rag(&empty, &sha), dir.path())
                .embed("a")
                .unwrap_err()
        );
        let (headers, sha) = write_model(dir.path(), "h.vec", "0 3\n");
        assert_eq!(
            AppError::InvalidState(format!(
                "No vectors were loaded from model: {}",
                headers.display()
            )),
            adapter(rag(&headers, &sha), dir.path())
                .embed("a")
                .unwrap_err()
        );
    }

    #[test]
    fn unparsable_numbers_become_zero_and_java_float_syntax_is_accepted() {
        assert_eq!(0.0, parse_java_float("abc"));
        assert_eq!(0.0, parse_java_float("inf"));
        assert_eq!(0.0, parse_java_float(""));
        assert_eq!(1.5, parse_java_float("1.5f"));
        assert_eq!(-2.0, parse_java_float("-2"));
        assert_eq!(1e-3, parse_java_float("1E-3"));
        assert!(parse_java_float("NaN").is_nan());
        assert_eq!(f32::INFINITY, parse_java_float("Infinity"));
    }

    #[test]
    fn download_with_wrong_checksum_leaves_no_model_file() {
        let dir = tempfile::tempdir().unwrap();
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).path("/ko.vec");
            then.status(200).body("1 3\nhello 1 0 0\n");
        });
        let path = dir.path().join("models").join("ko.vec");
        let mut config = rag(&path, &sha256_hex(b"something else"));
        config.download_url = server.url("/ko.vec");
        config.download_retry_max = 2;
        let adapter = adapter(config, dir.path());

        let err = adapter.embed("hello").unwrap_err();

        assert_eq!(
            AppError::InvalidState(format!(
                "FastText model sha256 mismatch for {}",
                path.display()
            )),
            err
        );
        assert_eq!(1, mock.hits(), "체크섬 불일치는 다시 받지 않는다");
        assert!(!path.exists());
        assert!(dir_is_empty(path.parent().unwrap()));
    }

    #[test]
    fn truncated_download_leaves_no_model_file() {
        let dir = tempfile::tempdir().unwrap();
        let full = "2 3\nhello 1 0 0\nworld 0 1 0\n";
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/ko.vec");
            then.status(200).body(&full[..full.len() - 10]);
        });
        let path = dir.path().join("ko.vec");
        let mut config = rag(&path, &sha256_hex(full.as_bytes()));
        config.download_url = server.url("/ko.vec");
        let adapter = adapter(config, dir.path());

        let err = adapter.embed("hello").unwrap_err();

        assert!(matches!(err, AppError::InvalidState(m) if m.contains("sha256 mismatch")));
        assert!(!path.exists());
        assert!(dir_is_empty(dir.path()));
    }

    /// 헤더는 바로 보내고 본문을 조금씩 흘리는 서버. `stall_after_bytes`가 있으면 그 뒤로는 멈춘다.
    fn dribble_server(
        body: &'static [u8],
        stall_after_bytes: Option<usize>,
    ) -> (String, std::thread::JoinHandle<()>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/ko.vec", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 2048];
            let _ = stream.read(&mut request);
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(head.as_bytes()).unwrap();
            for (sent, chunk) in body.chunks(2).enumerate() {
                if stall_after_bytes.is_some_and(|limit| sent * 2 >= limit) {
                    std::thread::sleep(Duration::from_secs(3));
                    return;
                }
                if stream.write_all(chunk).is_err() {
                    return;
                }
                let _ = stream.flush();
                std::thread::sleep(Duration::from_millis(150));
            }
        });
        (url, handle)
    }

    #[test]
    fn slow_body_longer_than_the_configured_timeout_still_downloads() {
        let dir = tempfile::tempdir().unwrap();
        let body: &'static str = "2 3\nhello 1 0 0\nworld 0 1 0\n"; // 약 2초 걸린다
        let (url, server) = dribble_server(body.as_bytes(), None);
        let path = dir.path().join("ko.vec");
        let mut config = rag(&path, &sha256_hex(body.as_bytes()));
        config.download_url = url;
        config.download_timeout_seconds = 1;
        let adapter = adapter(config, dir.path());

        let started = std::time::Instant::now();
        let vector = adapter.embed("hello").unwrap();

        assert!(started.elapsed() > Duration::from_secs(1));
        assert_eq!(vec![1.0, 0.0, 0.0], vector);
        assert!(path.exists());
        server.join().unwrap();
    }

    #[test]
    fn stalled_body_fails_after_the_stall_limit_and_leaves_no_file() {
        let dir = tempfile::tempdir().unwrap();
        let body: &'static str = "2 3\nhello 1 0 0\nworld 0 1 0\n";
        let (url, server) = dribble_server(body.as_bytes(), Some(8));
        let path = dir.path().join("ko.vec");
        let mut config = rag(&path, &sha256_hex(body.as_bytes()));
        config.download_url = url;
        config.download_timeout_seconds = 5;
        let adapter = adapter(config, dir.path()).with_stall_timeout(Duration::from_millis(700));

        let err = adapter.embed("hello").unwrap_err();

        assert_eq!(
            AppError::InvalidState(
                "Failed to download FastText model after retries: Model download failed: stalled: no data received"
                    .to_string()
            ),
            err
        );
        assert!(!path.exists());
        server.join().unwrap();
        // 받기 스레드는 임시 파일을 놓은 뒤에 결과를 보낸다.
        assert!(dir_is_empty(dir.path()));
    }

    #[test]
    fn failed_download_is_retried_and_leaves_no_file() {
        let dir = tempfile::tempdir().unwrap();
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).path("/ko.vec");
            then.status(500);
        });
        let path = dir.path().join("ko.vec");
        let mut config = rag(&path, &sha256_hex(b"x"));
        config.download_url = server.url("/ko.vec");
        config.download_retry_max = 1;
        let adapter = adapter(config, dir.path());

        let err = adapter.embed("hello").unwrap_err();

        assert_eq!(2, mock.hits());
        assert_eq!(
            AppError::InvalidState(
                "Failed to download FastText model after retries: Model download failed with status 500"
                    .to_string()
            ),
            err
        );
        assert!(dir_is_empty(dir.path()));
    }

    #[test]
    fn missing_checksum_or_url_fails_before_any_download() {
        let dir = tempfile::tempdir().unwrap();
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).path("/ko.vec");
            then.status(200).body("x");
        });
        let path = dir.path().join("ko.vec");

        let mut no_sha = rag(&path, "  ");
        no_sha.download_url = server.url("/ko.vec");
        assert_eq!(
            AppError::InvalidState(
                "omypic.rag.model-sha256 is required for fasttext provider".to_string()
            ),
            adapter(no_sha, dir.path()).embed("a").unwrap_err()
        );

        let no_url = rag(&path, "abc");
        assert_eq!(
            AppError::InvalidState(
                "FastText model is missing and omypic.rag.download-url is not configured"
                    .to_string()
            ),
            adapter(no_url, dir.path()).embed("a").unwrap_err()
        );
        assert_eq!(0, mock.hits());
    }

    #[test]
    fn default_model_path_uses_data_dir_models_and_model_version() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = rag(Path::new("x"), "abc");
        config.model_path = " ".to_string();
        config.model_version = " cc.ko.300.vec.gz ".to_string();
        let adapter = adapter(config, dir.path());
        assert_eq!(
            dir.path().join("models").join("cc.ko.300.vec.gz"),
            adapter.resolve_model_path()
        );
        assert_eq!("cc.ko.300.vec.gz", adapter.model_version());
        assert_eq!("fasttext", adapter.provider());

        let mut blank = rag(Path::new("x"), "abc");
        blank.model_path = String::new();
        blank.model_version = String::new();
        let blank = FastTextEmbeddingAdapter::new(
            blank,
            Arc::new(FixedDataDir(dir.path().to_string_lossy().into())),
        );
        assert_eq!(
            dir.path().join("models").join("fasttext.vec.gz"),
            blank.resolve_model_path()
        );
        assert_eq!("unknown-model", blank.model_version());
    }

    #[test]
    fn embed_cache_is_bounded_and_evicts_least_recently_used() {
        let mut cache = EmbedCache::default();
        for i in 0..EMBED_CACHE_MAX_ENTRIES {
            cache.put(format!("k{i}"), vec![i as f32]);
        }
        assert!(cache.get("k0").is_some()); // k0은 방금 읽어서 가장 최근이다.
        cache.put("new".to_string(), vec![0.0]);
        assert_eq!(EMBED_CACHE_MAX_ENTRIES, cache.entries.len());
        assert!(cache.get("k1").is_none());
        assert!(cache.get("k0").is_some());
    }

    #[test]
    fn a_missing_model_is_loaded_in_the_background_once_the_file_appears() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ko.vec");
        let adapter = adapter(rag(&path, &sha256_hex(MODEL.as_bytes())), dir.path());
        let missing = adapter.embed("hello").unwrap_err();
        assert!(matches!(&missing, AppError::InvalidState(m) if m.contains("download-url")));
        // Nothing heals this by itself: no background retries until the file changes.
        assert!(!adapter.worker_running());
        assert_eq!(missing, adapter.embed("hello").unwrap_err());

        // The user fixes the setup (here: the model file appears).
        std::fs::write(&path, MODEL).unwrap();

        // The call that notices returns at once with the remembered failure ...
        assert_eq!(missing, adapter.embed("hello").unwrap_err());
        // ... and a background load makes the model available to the next calls.
        assert!(eventually(Duration::from_secs(10), || adapter
            .embed("hello")
            .is_ok_and(|v| v == vec![1.0, 0.0, 0.0])));
    }

    #[test]
    fn hash_fallback_vectors_are_not_cached_while_the_model_is_unavailable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ko.vec");
        let mut config = rag(&path, &sha256_hex(MODEL.as_bytes()));
        config.allow_hash_fallback = true;
        let adapter = adapter(config, dir.path());
        assert_eq!(
            LocalHashEmbeddingAdapter::new(3).embed_text("hello"),
            adapter.embed("hello").unwrap()
        );

        std::fs::write(&path, MODEL).unwrap();

        assert!(eventually(Duration::from_secs(10), || adapter
            .embed("hello")
            .unwrap()
            == vec![1.0, 0.0, 0.0]));
    }

    #[test]
    fn after_a_failed_load_callers_get_the_failure_at_once_while_one_loader_retries() {
        let dir = tempfile::tempdir().unwrap();
        let body = "2 3\nhello 1 0 0\nworld 0 1 0\n";
        let server = MockServer::start();
        let mut failing = server.mock(|when, then| {
            when.method(GET).path("/ko.vec");
            then.status(500).delay(Duration::from_millis(800));
        });
        let path = dir.path().join("ko.vec");
        let mut config = rag(&path, &sha256_hex(body.as_bytes()));
        config.download_url = server.url("/ko.vec");
        let adapter = adapter(config, dir.path())
            .with_retry_backoff(Duration::from_millis(100), Duration::from_millis(400));

        // The first load of the process blocks the caller, as before.
        let started = Instant::now();
        let first = adapter.embed("hello").unwrap_err();
        assert!(started.elapsed() >= Duration::from_millis(800));
        assert!(adapter.worker_running());

        // Later callers never wait for a retry (each retry takes 800 ms here).
        for _ in 0..10 {
            let started = Instant::now();
            assert_eq!(first, adapter.embed("hello").unwrap_err());
            assert!(
                started.elapsed() < Duration::from_millis(300),
                "{:?}",
                started.elapsed()
            );
            std::thread::sleep(Duration::from_millis(100));
        }
        // One loader, growing back-off: 2 s of failures are at most a handful of requests.
        let hits = failing.hits();
        assert!((2..=4).contains(&hits), "hits={hits}");

        failing.delete();
        server.mock(|when, then| {
            when.method(GET).path("/ko.vec");
            then.status(200).body(body);
        });
        assert!(eventually(Duration::from_secs(10), || adapter
            .embed("hello")
            .is_ok()));
        assert_eq!(vec![0.0, 1.0, 0.0], adapter.embed("world").unwrap());
        assert!(eventually(Duration::from_secs(2), || !adapter.worker_running()));
    }

    #[test]
    fn a_completed_download_with_the_wrong_checksum_is_not_downloaded_again() {
        let dir = tempfile::tempdir().unwrap();
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).path("/ko.vec");
            then.status(200).body("1 3\nhello 0 1 0\n");
        });
        let path = dir.path().join("ko.vec");
        let mut config = rag(&path, &sha256_hex(MODEL.as_bytes()));
        config.download_url = server.url("/ko.vec");
        let adapter = adapter(config, dir.path())
            .with_retry_backoff(Duration::from_millis(10), Duration::from_millis(10));

        let failure = adapter.embed("hello").unwrap_err();
        assert!(matches!(&failure, AppError::InvalidState(m) if m.contains("sha256 mismatch")));
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(failure, adapter.embed("hello").unwrap_err());
        assert_eq!(
            1,
            mock.hits(),
            "a checksum mismatch does not heal by downloading again"
        );
        assert!(!adapter.worker_running());

        // A correct file put in place by the user is picked up.
        std::fs::write(&path, MODEL).unwrap();
        assert!(eventually(Duration::from_secs(10), || adapter
            .embed("hello")
            .is_ok()));
        assert_eq!(1, mock.hits());
    }

    #[test]
    fn a_file_with_the_wrong_dimension_is_not_read_again_until_it_changes() {
        let dir = tempfile::tempdir().unwrap();
        let (path, sha) = write_model(dir.path(), "ko.vec", "2 4\nhello 1 0 0 0\n");
        let adapter = adapter(rag(&path, &sha), dir.path())
            .with_retry_backoff(Duration::from_millis(10), Duration::from_millis(10));

        let failure = adapter.embed("hello").unwrap_err();
        assert!(!adapter.worker_running());
        // Removing the file and putting it back unchanged keeps the failure: same size and time.
        let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
        let content = std::fs::read(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        std::fs::write(&path, &content).unwrap();
        File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(modified)
            .unwrap();
        assert_eq!(failure, adapter.embed("hello").unwrap_err());
        assert!(!adapter.worker_running());
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(failure, adapter.embed("hello").unwrap_err());
        assert!(!adapter.worker_running());
    }

    #[test]
    fn loaded_vectors_are_released_after_the_idle_period_and_read_again_on_demand() {
        let dir = tempfile::tempdir().unwrap();
        let (path, sha) = write_model(dir.path(), "ko.vec", MODEL);
        let adapter = adapter(rag(&path, &sha), dir.path())
            .with_idle_release(Some(Duration::from_millis(1500)));

        assert_eq!(vec![1.0, 0.0, 0.0], adapter.embed("hello").unwrap());
        assert!(adapter.is_loaded());
        // Each embedding request keeps the model: here one every 300 ms for 2.1 s, longer than
        // the idle period, with 1.2 s to spare between two requests.
        for word in [
            "world",
            "alpha",
            "world alpha",
            "hello world",
            "alpha hello",
            "x y",
            "a b",
        ] {
            adapter.embed(word).unwrap();
            std::thread::sleep(Duration::from_millis(300));
            assert!(adapter.is_loaded(), "released while in use");
        }

        assert!(eventually(Duration::from_secs(5), || !adapter.is_loaded()));
        assert!(eventually(Duration::from_secs(2), || !adapter.worker_running()));

        // Not cached yet: needs the model, which is read again.
        assert_eq!(
            vec![0.0, 0.707_106_77, 0.707_106_77],
            adapter.embed("world, alpha!").unwrap()
        );
        assert!(adapter.is_loaded());
    }

    #[test]
    fn without_an_idle_period_the_vectors_stay_and_no_worker_runs() {
        let dir = tempfile::tempdir().unwrap();
        let (path, sha) = write_model(dir.path(), "ko.vec", MODEL);
        let adapter = adapter(rag(&path, &sha), dir.path());

        adapter.embed("hello").unwrap();

        assert!(adapter.is_loaded());
        assert!(!adapter.worker_running());
    }

    #[test]
    fn dropping_the_adapter_stops_the_background_loader() {
        let dir = tempfile::tempdir().unwrap();
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).path("/ko.vec");
            then.status(500);
        });
        let path = dir.path().join("ko.vec");
        let mut config = rag(&path, &sha256_hex(MODEL.as_bytes()));
        config.download_url = server.url("/ko.vec");
        let adapter = adapter(config, dir.path())
            .with_retry_backoff(Duration::from_millis(20), Duration::from_millis(20));
        assert!(adapter.embed("hello").is_err());
        assert!(eventually(Duration::from_secs(5), || mock.hits() >= 3));

        drop(adapter);
        std::thread::sleep(Duration::from_millis(200));
        let after_drop = mock.hits();
        std::thread::sleep(Duration::from_millis(500));

        assert_eq!(
            after_drop,
            mock.hits(),
            "the loader kept retrying after the drop"
        );
    }

    /// 첫 연결에는 헤더와 본문 몇 바이트만 보내고 멈춘 뒤, 클라이언트가 연결을 놓는지 본다
    /// (`released`). 두 번째 연결에는 본문을 다 보낸다.
    fn stall_then_serve_server(
        body: &'static [u8],
    ) -> (
        String,
        std::sync::mpsc::Receiver<bool>,
        std::thread::JoinHandle<()>,
    ) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/ko.vec", listener.local_addr().unwrap());
        let (released_tx, released_rx) = std::sync::mpsc::channel();
        let handle = std::thread::spawn(move || {
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let (mut stalled, _) = listener.accept().unwrap();
            let mut request = [0_u8; 2048];
            let _ = stalled.read(&mut request);
            stalled.write_all(head.as_bytes()).unwrap();
            stalled.write_all(&body[..4]).unwrap();
            stalled.flush().unwrap();
            stalled
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut rest = [0_u8; 16];
            // 0 bytes or a reset: the client closed its side. A timeout: it is still holding on.
            let released = !matches!(
                stalled.read(&mut rest),
                Err(ref err) if err.kind() == std::io::ErrorKind::WouldBlock
                    || err.kind() == std::io::ErrorKind::TimedOut
            );
            let _ = released_tx.send(released);

            let (mut served, _) = listener.accept().unwrap();
            let _ = served.read(&mut request);
            served.write_all(head.as_bytes()).unwrap();
            served.write_all(body).unwrap();
        });
        (url, released_rx, handle)
    }

    #[test]
    fn stalled_download_releases_its_connection_and_is_retried_later() {
        let dir = tempfile::tempdir().unwrap();
        let body: &'static str = "2 3\nhello 1 0 0\nworld 0 1 0\n";
        let (url, released, server) = stall_then_serve_server(body.as_bytes());
        let path = dir.path().join("ko.vec");
        let mut config = rag(&path, &sha256_hex(body.as_bytes()));
        config.download_url = url;
        config.download_timeout_seconds = 1;
        let adapter = adapter(config, dir.path())
            .with_stall_timeout(Duration::from_millis(300))
            .with_retry_backoff(Duration::from_millis(50), Duration::from_millis(50));

        assert!(adapter.embed("hello").is_err());
        assert_eq!(
            Ok(true),
            released.recv_timeout(Duration::from_secs(10)),
            "the abandoned download worker must let go of the stalled connection"
        );

        assert!(eventually(Duration::from_secs(10), || adapter
            .embed("hello")
            .is_ok_and(|v| v == vec![1.0, 0.0, 0.0])));
        server.join().unwrap();
        assert_eq!(1, std::fs::read_dir(dir.path()).unwrap().count());
    }

    #[test]
    fn a_stalled_download_ends_within_the_stall_limit_even_with_the_long_default_header_limit() {
        let dir = tempfile::tempdir().unwrap();
        let body: &'static [u8] = b"2 3\nhello 1 0 0\nworld 0 1 0\n";
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/ko.vec", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut stalled, _) = listener.accept().unwrap();
            let mut request = [0_u8; 2048];
            let _ = stalled.read(&mut request);
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stalled.write_all(head.as_bytes()).unwrap();
            stalled.write_all(&body[..4]).unwrap();
            stalled.flush().unwrap();
            let stalled_at = Instant::now();
            stalled
                .set_read_timeout(Some(Duration::from_secs(20)))
                .unwrap();
            let mut rest = [0_u8; 16];
            // 0 bytes or a reset: the client closed its side.
            let closed = !matches!(
                stalled.read(&mut rest),
                Err(ref err) if err.kind() == std::io::ErrorKind::WouldBlock
                    || err.kind() == std::io::ErrorKind::TimedOut
            );
            (closed, stalled_at.elapsed())
        });
        let path = dir.path().join("models").join("ko.vec");
        let mut config = rag(&path, &sha256_hex(body));
        config.download_url = url;
        // The default relation: the header wait (30 minutes by default) is far longer than the
        // stall limit (60 seconds by default). Here 60 s against 300 ms.
        config.download_timeout_seconds = 60;
        let adapter = adapter(config, dir.path())
            .with_stall_timeout(Duration::from_millis(300))
            .with_retry_backoff(Duration::from_secs(60), Duration::from_secs(60));

        let started = Instant::now();
        assert!(adapter.embed("hello").is_err());
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "{:?}",
            started.elapsed()
        );

        let (closed, held_for) = server.join().unwrap();
        assert!(
            closed,
            "the download kept the stalled connection open for 20 s"
        );
        assert!(
            held_for < Duration::from_secs(5),
            "connection held for {held_for:?}"
        );
        assert!(
            dir_is_empty(path.parent().unwrap()),
            "the partial download was left behind"
        );
    }

    #[test]
    fn dropping_the_adapter_cancels_a_download_in_progress_and_removes_its_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let body: &'static [u8] = b"2 3\nhello 1 0 0\nworld 0 1 0\n";
        // The caller's attempt gets a 500; the background retry gets a few bytes and then
        // nothing (stall limit: 60 s). The server reports whether the client let go of it.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/ko.vec", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let mut request = [0_u8; 2048];
            let (mut refused, _) = listener.accept().unwrap();
            let _ = refused.read(&mut request);
            refused
                .write_all(b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .unwrap();
            drop(refused);
            let (mut stalled, _) = listener.accept().unwrap();
            let _ = stalled.read(&mut request);
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stalled.write_all(head.as_bytes()).unwrap();
            stalled.write_all(&body[..4]).unwrap();
            stalled.flush().unwrap();
            stalled
                .set_read_timeout(Some(Duration::from_secs(20)))
                .unwrap();
            let mut rest = [0_u8; 16];
            // 0 bytes or a reset: the client closed its side.
            !matches!(
                stalled.read(&mut rest),
                Err(ref err) if err.kind() == std::io::ErrorKind::WouldBlock
                    || err.kind() == std::io::ErrorKind::TimedOut
            )
        });
        let path = dir.path().join("models").join("ko.vec");
        let mut config = rag(&path, &sha256_hex(body));
        config.download_url = url;
        let adapter = adapter(config, dir.path())
            .with_retry_backoff(Duration::from_millis(50), Duration::from_millis(50));
        assert!(adapter.embed("hello").is_err());
        // The background retry is writing its temp file.
        assert!(eventually(Duration::from_secs(5), || std::fs::read_dir(
            path.parent().unwrap()
        )
        .map(|entries| entries.count() == 1)
        .unwrap_or(false)));

        let started = Instant::now();
        drop(adapter);

        assert!(
            server.join().unwrap(),
            "the download kept the connection after the adapter was dropped"
        );
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "{:?}",
            started.elapsed()
        );
        assert!(eventually(Duration::from_secs(5), || dir_is_empty(
            path.parent().unwrap()
        )));
    }

    /// Serves `body` to every GET and counts the requests. From request number `hold_from` on
    /// (1-based) it holds the reply until `open` is called.
    struct GatedUpstream {
        url: String,
        requests: Arc<std::sync::atomic::AtomicUsize>,
        gate: Arc<(Mutex<bool>, Condvar)>,
    }

    impl GatedUpstream {
        fn start(body: &'static [u8], hold_from: usize) -> Self {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://{}/ko.vec", listener.local_addr().unwrap());
            let requests = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let gate = Arc::new((Mutex::new(false), Condvar::new()));
            let (counter, held) = (Arc::clone(&requests), Arc::clone(&gate));
            std::thread::spawn(move || {
                for stream in listener.incoming() {
                    let Ok(mut stream) = stream else { return };
                    let (counter, held) = (Arc::clone(&counter), Arc::clone(&held));
                    std::thread::spawn(move || {
                        let mut request = Vec::new();
                        let mut byte = [0_u8; 1];
                        while !request.ends_with(b"\r\n\r\n") {
                            match stream.read(&mut byte) {
                                Ok(1) => request.push(byte[0]),
                                _ => return,
                            }
                        }
                        let number = counter.fetch_add(1, Ordering::SeqCst) + 1;
                        if number >= hold_from {
                            let (open, wake) = &*held;
                            let mut open = lock(open);
                            while !*open {
                                open = wake.wait(open).unwrap();
                            }
                        }
                        let head = format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        );
                        let _ = stream.write_all(head.as_bytes());
                        let _ = stream.write_all(body);
                    });
                }
            });
            Self {
                url,
                requests,
                gate,
            }
        }

        fn requests(&self) -> usize {
            self.requests.load(Ordering::SeqCst)
        }

        fn open(&self) {
            *lock(&self.gate.0) = true;
            self.gate.1.notify_all();
        }
    }

    impl Drop for GatedUpstream {
        fn drop(&mut self) {
            self.open();
        }
    }

    #[test]
    fn after_an_idle_release_a_missing_file_fails_the_caller_at_once_and_is_downloaded_in_the_background(
    ) {
        let dir = tempfile::tempdir().unwrap();
        let body: &'static str = "2 3\nhello 1 0 0\nworld 0 1 0\n";
        let upstream = GatedUpstream::start(body.as_bytes(), 2);
        let path = dir.path().join("models").join("ko.vec");
        let mut config = rag(&path, &sha256_hex(body.as_bytes()));
        config.download_url = upstream.url.clone();
        let adapter = Arc::new(
            adapter(config, dir.path())
                .with_idle_release(Some(Duration::from_millis(200)))
                .with_retry_backoff(Duration::from_millis(50), Duration::from_millis(50)),
        );

        // The first load of the process downloads in the caller.
        assert_eq!(vec![1.0, 0.0, 0.0], adapter.embed("hello").unwrap());
        assert_eq!(1, upstream.requests());
        assert!(eventually(Duration::from_secs(5), || !adapter.is_loaded()));
        std::fs::remove_file(&path).unwrap();

        // The upstream holds every further request until it is opened: a caller that downloaded
        // by itself would not come back.
        let (sender, receiver) = std::sync::mpsc::channel();
        {
            let adapter = Arc::clone(&adapter);
            std::thread::spawn(move || {
                let _ = sender.send(adapter.embed("world"));
            });
        }
        let result = receiver
            .recv_timeout(Duration::from_secs(5))
            .expect("the caller waited for a download after the idle release");
        assert!(
            matches!(&result, Err(AppError::InvalidState(m)) if m.contains("missing")),
            "{result:?}"
        );

        // The background worker asks the upstream; once it answers, the model is back.
        assert!(eventually(Duration::from_secs(5), || upstream.requests() == 2));
        upstream.open();
        assert!(eventually(Duration::from_secs(10), || adapter
            .embed("world")
            .is_ok_and(|v| v == vec![0.0, 1.0, 0.0])));
        assert!(path.exists());
        assert_eq!(2, upstream.requests());
    }

    #[test]
    fn a_file_replaced_while_it_is_read_is_checked_again_on_the_next_load() {
        let dir = tempfile::tempdir().unwrap();
        let (path, sha) = write_model(dir.path(), "ko.vec", MODEL);
        let adapter = adapter(rag(&path, &sha), dir.path())
            .with_idle_release(Some(Duration::from_millis(100)));
        // Replaced (other size) after the checksum was computed, before the vectors are read.
        let replaced = path.clone();
        *lock(&adapter.inner.after_checksum) = Some(Box::new(move || {
            std::fs::write(&replaced, "2 3\nhello 0 1 0\nworld 1 0 0\n").unwrap();
        }));
        adapter.embed("hello").unwrap();
        assert!(eventually(Duration::from_secs(5), || !adapter.is_loaded()));

        // The replacement never passed the checksum, so the next load computes it.
        assert_eq!(
            AppError::InvalidState(format!(
                "FastText model sha256 mismatch for {}",
                path.display()
            )),
            adapter.embed("world").unwrap_err()
        );
    }

    #[test]
    fn a_worker_that_failed_to_start_is_started_by_the_next_call_to_release_the_model() {
        let dir = tempfile::tempdir().unwrap();
        let (path, sha) = write_model(dir.path(), "ko.vec", MODEL);
        let adapter = adapter(rag(&path, &sha), dir.path())
            .with_idle_release(Some(Duration::from_millis(100)));
        // Every attempt of the first call to start the worker fails.
        adapter.inner.failing_spawns.store(2, Ordering::SeqCst);

        adapter.embed("hello").unwrap();
        assert!(!adapter.worker_running());
        adapter.embed("world").unwrap();

        assert!(eventually(Duration::from_secs(5), || !adapter.is_loaded()));
    }

    #[test]
    fn a_worker_that_failed_to_start_is_started_by_the_next_call_to_retry_the_load() {
        let dir = tempfile::tempdir().unwrap();
        let body = "2 3\nhello 1 0 0\nworld 0 1 0\n";
        let server = MockServer::start();
        let mut failing = server.mock(|when, then| {
            when.method(GET).path("/ko.vec");
            then.status(500);
        });
        let path = dir.path().join("ko.vec");
        let mut config = rag(&path, &sha256_hex(body.as_bytes()));
        config.download_url = server.url("/ko.vec");
        let adapter = adapter(config, dir.path())
            .with_retry_backoff(Duration::from_millis(50), Duration::from_millis(50));
        // Every attempt of the first call to start the worker fails.
        adapter.inner.failing_spawns.store(2, Ordering::SeqCst);

        assert!(adapter.embed("hello").is_err());
        assert!(!adapter.worker_running());
        failing.delete();
        server.mock(|when, then| {
            when.method(GET).path("/ko.vec");
            then.status(200).body(body);
        });

        assert!(eventually(Duration::from_secs(10), || adapter
            .embed("hello")
            .is_ok()));
    }

    /// Runs `f` on this thread and counts the log lines that contain `needle`.
    fn count_logs(needle: &'static str, f: impl FnOnce()) -> usize {
        #[derive(Clone)]
        struct Counter(Arc<std::sync::atomic::AtomicUsize>, &'static str);
        impl Write for Counter {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                if String::from_utf8_lossy(bytes).contains(self.1) {
                    self.0.fetch_add(1, Ordering::SeqCst);
                }
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let counter = Counter(Arc::new(std::sync::atomic::AtomicUsize::new(0)), needle);
        let writer = counter.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(move || writer.clone())
            .with_ansi(false)
            .finish();
        // 다른 시험 스레드가 구독자 없이 먼저 지나간 로그 자리는 "관심 없음"으로 기억될 수 있다.
        tracing::subscriber::with_default(subscriber, || {
            tracing::callsite::rebuild_interest_cache();
            f()
        });
        counter.0.load(Ordering::SeqCst)
    }

    #[test]
    fn the_hash_fallback_warning_is_logged_once_per_failure() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ko.vec");
        let mut config = rag(&path, &sha256_hex(MODEL.as_bytes()));
        config.allow_hash_fallback = true;
        let adapter = adapter(config, dir.path());

        let warnings = count_logs("using hash fallback", || {
            for text in ["one", "two", "three", "four", "five"] {
                adapter.embed(text).unwrap();
            }
        });
        assert_eq!(1, warnings);

        // A new failure (the file appears with the wrong content) is reported again, once.
        std::fs::write(&path, "1 3\nhello 0 1 0\n").unwrap();
        adapter.embed("six").unwrap();
        assert!(eventually(Duration::from_secs(5), || !adapter.worker_running()));
        let warnings = count_logs("using hash fallback", || {
            for text in ["seven", "eight", "nine"] {
                adapter.embed(text).unwrap();
            }
        });
        assert_eq!(1, warnings);
    }

    #[test]
    fn stale_partial_downloads_are_removed_and_fresh_ones_kept() {
        let dir = tempfile::tempdir().unwrap();
        let stale = dir.path().join(".model-download-old.part");
        let fresh = dir.path().join(".model-download-new.part");
        let other = dir.path().join("notes.part");
        for file in [&stale, &fresh, &other] {
            std::fs::write(file, b"x").unwrap();
        }
        let old = SystemTime::now() - Duration::from_secs(600);
        File::options()
            .write(true)
            .open(&stale)
            .unwrap()
            .set_modified(old)
            .unwrap();

        remove_stale_temp_files(dir.path(), Duration::from_secs(120));

        assert!(!stale.exists());
        assert!(fresh.exists());
        assert!(other.exists());
    }
}
