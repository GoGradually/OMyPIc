//! fastText `.vec`/`.vec.gz` 모델을 읽어 토큰 벡터의 평균(합 후 정규화)으로 임베딩한다.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

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

type Vectors = HashMap<String, Vec<f32>>;

enum ModelState {
    Pending,
    Ready(Arc<Vectors>),
    Failed(AppError),
}

pub struct FastTextEmbeddingAdapter {
    rag: RagConfig,
    data_dir: Arc<dyn DataDirProvider>,
    hash_fallback: LocalHashEmbeddingAdapter,
    cache: Mutex<EmbedCache>,
    state: Mutex<ModelState>,
    /// 본문이 이만큼 한 바이트도 안 오면 내려받기를 실패로 본다.
    stall_timeout: Duration,
}

impl FastTextEmbeddingAdapter {
    pub fn new(rag: RagConfig, data_dir: Arc<dyn DataDirProvider>) -> Self {
        let hash_fallback = LocalHashEmbeddingAdapter::new(rag.embedding_dim);
        Self {
            rag,
            data_dir,
            hash_fallback,
            cache: Mutex::new(EmbedCache::default()),
            state: Mutex::new(ModelState::Pending),
            stall_timeout: Duration::from_secs(DOWNLOAD_STALL_SECONDS),
        }
    }

    #[cfg(test)]
    fn with_stall_timeout(mut self, stall_timeout: Duration) -> Self {
        self.stall_timeout = stall_timeout;
        self
    }

    fn dim(&self) -> usize {
        self.rag.embedding_dim
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
        if self.rag.allow_hash_fallback {
            return self.hash_fallback.embed_text(text);
        }
        sum
    }

    /// `Ok(None)`이면 모델을 쓸 수 없지만 해시 대체가 허용된 상태다.
    fn ensure_vectors(&self) -> AppResult<Option<Arc<Vectors>>> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let ModelState::Pending = *state {
            *state = match self.load_model() {
                Ok(vectors) => ModelState::Ready(Arc::new(vectors)),
                Err(err) => ModelState::Failed(err),
            };
        }
        match &*state {
            ModelState::Ready(vectors) => Ok(Some(vectors.clone())),
            ModelState::Failed(failure) => self.handle_initialization_failure(failure),
            ModelState::Pending => unreachable!("모델 상태는 위에서 정해졌다"),
        }
    }

    fn handle_initialization_failure(&self, failure: &AppError) -> AppResult<Option<Arc<Vectors>>> {
        if self.rag.allow_hash_fallback {
            tracing::warn!("FastText model unavailable; using hash fallback. reason={failure}");
            return Ok(None);
        }
        Err(failure.clone())
    }

    fn load_model(&self) -> AppResult<Vectors> {
        let model_path = self.ensure_model_file()?;
        let vectors = self.load_vectors(&model_path)?;
        tracing::info!(
            "RAG fasttext model loaded path={} vocab={}",
            model_path.display(),
            vectors.len()
        );
        Ok(vectors)
    }

    fn ensure_model_file(&self) -> AppResult<PathBuf> {
        let model_path = self.resolve_model_path();
        if let Some(parent) = model_path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| {
                AppError::InvalidState(format!(
                    "Failed to prepare model directory: {}: {err}",
                    model_path.display()
                ))
            })?;
        }
        if model_path.exists() {
            self.verify_model_sha256(&model_path)?;
            return Ok(model_path);
        }
        self.download_model_with_retry(&model_path)?;
        Ok(model_path)
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
        let actual = sha256_file(model_path)?;
        check_sha256(expected, &actual, model_path)
    }

    fn download_model_with_retry(&self, model_path: &Path) -> AppResult<()> {
        let url = java_trim(&self.rag.download_url);
        if url.is_empty() {
            return Err(AppError::InvalidState(
                "FastText model is missing and omypic.rag.download-url is not configured"
                    .to_string(),
            ));
        }
        // 내려받기 전에 확인한다. 체크섬 없이 받은 파일은 어차피 쓸 수 없다.
        let expected = self.expected_sha256()?;
        let attempts = self.rag.download_retry_max.saturating_add(1).max(1);
        let mut last_failure = String::new();
        for attempt in 1..=attempts {
            match self.download_model(model_path, url, expected) {
                Ok(()) => return Ok(()),
                Err(DownloadError::Fatal(err)) => return Err(err),
                Err(DownloadError::Transfer(message)) => {
                    last_failure = message;
                    if attempt < attempts {
                        std::thread::sleep(backoff(attempt));
                    }
                }
            }
        }
        Err(AppError::InvalidState(format!(
            "Failed to download FastText model after retries: {last_failure}"
        )))
    }

    /// 같은 디렉터리의 임시 파일에 받고, 체크섬이 맞을 때만 이름을 바꿔 넣는다.
    ///
    /// `download-timeout-seconds`는 연결과 응답 헤더를 기다리는 시간만 제한한다(Java와 같다).
    /// 본문은 느려도 계속 오는 한 끊지 않고, `stall_timeout` 동안 한 바이트도 안 오면 실패한다.
    /// reqwest 블로킹 클라이언트에는 읽기 제한 시간이 없어서 받는 일을 별도 스레드에서 하고
    /// 이 스레드가 진행 신호를 기다린다. 시간이 지나면 취소 표시를 남기고 포기한다.
    fn download_model(
        &self,
        model_path: &Path,
        url: &str,
        expected: &str,
    ) -> Result<(), DownloadError> {
        let header_timeout = Duration::from_secs(self.rag.download_timeout_seconds.max(1));
        let stall_timeout = self.stall_timeout;
        let (sender, receiver) = std::sync::mpsc::channel::<DownloadEvent>();
        let cancel = Arc::new(AtomicBool::new(false));
        let job = DownloadJob {
            url: url.to_string(),
            model_path: model_path.to_path_buf(),
            expected: expected.to_string(),
            connect_timeout: header_timeout,
            cancel: cancel.clone(),
        };
        std::thread::spawn(move || {
            let done = job.run(&sender);
            let _ = sender.send(DownloadEvent::Done(done));
        });

        let mut wait = header_timeout;
        loop {
            match receiver.recv_timeout(wait) {
                Ok(DownloadEvent::Started) | Ok(DownloadEvent::Progress) => wait = stall_timeout,
                Ok(DownloadEvent::Done(result)) => return result,
                Err(_) => {
                    cancel.store(true, Ordering::SeqCst);
                    let what = if wait == header_timeout {
                        "timed out waiting for response headers"
                    } else {
                        "stalled: no data received"
                    };
                    return Err(DownloadError::Transfer(format!(
                        "Model download failed: {what}"
                    )));
                }
            }
        }
    }

    fn load_vectors(&self, model_path: &Path) -> AppResult<Vectors> {
        let load_error = |err: std::io::Error| {
            AppError::InvalidState(format!(
                "Failed to load FastText vectors from {}: {err}",
                model_path.display()
            ))
        };
        let max_vocab = self.rag.model_max_vocab.max(1);
        let mut loaded = Vectors::new();
        let mut lines = LineReader::new(open_reader(model_path).map_err(load_error)?);

        let Some(first_line) = lines.next_line().map_err(load_error)? else {
            return Err(AppError::InvalidState(format!(
                "FastText model is empty: {}",
                model_path.display()
            )));
        };
        let header_dim = parse_header_dimension(&first_line)?;
        self.validate_dimension(header_dim)?;
        if header_dim.is_none() {
            self.parse_vector_line(&first_line, &mut loaded);
        }
        while loaded.len() < max_vocab {
            let Some(line) = lines.next_line().map_err(load_error)? else {
                break;
            };
            self.parse_vector_line(&line, &mut loaded);
        }
        if loaded.is_empty() {
            return Err(AppError::InvalidState(format!(
                "No vectors were loaded from model: {}",
                model_path.display()
            )));
        }
        Ok(loaded)
    }

    fn validate_dimension(&self, header_dim: Option<i32>) -> AppResult<()> {
        match header_dim {
            Some(actual) if i64::from(actual) != self.dim() as i64 => {
                Err(AppError::InvalidState(format!(
                    "Embedding dimension mismatch. expected={} actual={actual}",
                    self.dim()
                )))
            }
            _ => Ok(()),
        }
    }

    fn parse_vector_line(&self, line: &str, loaded: &mut Vectors) {
        let trimmed = java_trim(line);
        if trimmed.is_empty() {
            return;
        }
        let parts: Vec<&str> = trimmed
            .split(is_regex_space)
            .filter(|part| !part.is_empty())
            .collect();
        if parts.len() < self.dim() + 1 {
            return;
        }
        let mut vector: Vec<f32> = parts[1..=self.dim()]
            .iter()
            .map(|part| parse_java_float(part))
            .collect();
        normalize(&mut vector);
        loaded.entry(parts[0].to_string()).or_insert(vector);
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
            None => self.hash_fallback.embed_text(text),
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
        match java_trim(&self.rag.model_version) {
            "" => "unknown-model".to_string(),
            version => version.to_string(),
        }
    }
}

impl FastTextEmbeddingAdapter {
    fn lock_cache(&self) -> std::sync::MutexGuard<'_, EmbedCache> {
        self.cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

enum DownloadEvent {
    /// 응답 헤더를 받았다.
    Started,
    /// 본문 조각을 받았다.
    Progress,
    Done(Result<(), DownloadError>),
}

struct DownloadJob {
    url: String,
    model_path: PathBuf,
    expected: String,
    connect_timeout: Duration,
    cancel: Arc<AtomicBool>,
}

impl DownloadJob {
    fn run(&self, events: &std::sync::mpsc::Sender<DownloadEvent>) -> Result<(), DownloadError> {
        let transfer = |err: &dyn std::fmt::Display| {
            DownloadError::Transfer(format!("Model download failed: {err}"))
        };
        let client = reqwest::blocking::Client::builder()
            .connect_timeout(self.connect_timeout)
            .timeout(None)
            .tcp_keepalive(Some(Duration::from_secs(30)))
            .build()
            .map_err(|err| transfer(&err))?;
        let mut response = client.get(&self.url).send().map_err(|err| transfer(&err))?;
        if !response.status().is_success() {
            return Err(DownloadError::Transfer(format!(
                "Model download failed with status {}",
                response.status().as_u16()
            )));
        }
        let _ = events.send(DownloadEvent::Started);

        let dir = self.model_path.parent().unwrap_or_else(|| Path::new("."));
        let mut temp = tempfile::Builder::new()
            .prefix(".model-download-")
            .suffix(".part")
            .tempfile_in(dir)
            .map_err(|err| transfer(&err))?;
        let mut hasher = Sha256::new();
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            if self.cancel.load(Ordering::SeqCst) {
                return Err(DownloadError::Transfer(
                    "Model download cancelled".to_string(),
                ));
            }
            let read = response.read(&mut buffer).map_err(|err| transfer(&err))?;
            if read == 0 {
                break;
            }
            let _ = events.send(DownloadEvent::Progress);
            hasher.update(&buffer[..read]);
            temp.write_all(&buffer[..read])
                .map_err(|err| transfer(&err))?;
        }
        temp.flush().map_err(|err| transfer(&err))?;

        check_sha256(
            &self.expected,
            &to_hex(&hasher.finalize()),
            &self.model_path,
        )
        .map_err(DownloadError::Fatal)?;
        if self.cancel.load(Ordering::SeqCst) {
            return Err(DownloadError::Transfer(
                "Model download cancelled".to_string(),
            ));
        }
        temp.persist(&self.model_path)
            .map_err(|err| transfer(&err.error))?;
        Ok(())
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

fn sha256_file(path: &Path) -> AppResult<String> {
    let read_error = |err: std::io::Error| {
        AppError::InvalidState(format!(
            "Failed to read model file for checksum: {}: {err}",
            path.display()
        ))
    };
    let mut file = File::open(path).map_err(read_error)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 8192];
    loop {
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

/// 첫 줄이 `<정수> <정수>` 두 토큰이면 헤더이고 둘째가 차원이다.
fn parse_header_dimension(line: &str) -> AppResult<Option<i32>> {
    let parts: Vec<&str> = java_trim(line)
        .split(is_regex_space)
        .filter(|part| !part.is_empty())
        .collect();
    if parts.len() != 2 || !parts.iter().all(|part| is_integer(part)) {
        return Ok(None);
    }
    parts[1]
        .parse::<i32>()
        .map(Some)
        .map_err(|_| AppError::InvalidArgument(format!("For input string: \"{}\"", parts[1])))
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
        // 취소된 받기 스레드가 임시 파일을 지울 때까지 잠깐 기다린다.
        std::thread::sleep(Duration::from_millis(500));
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
}
