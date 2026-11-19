use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use omypic_application::feedback::policy::FeedbackModelPolicy;
use omypic_application::feedback::port::{LlmClient, LlmGenerateResult};
use omypic_domain::error::{AppError, AppResult};
use omypic_domain::feedback::Feedback;
use omypic_domain::session::{LlmConversationState, LlmPromptContext};
use omypic_domain::shared::text_utils::{java_is_blank, java_trim};
use reqwest::blocking::Client;
use reqwest::header::HeaderMap;
use serde_json::{json, Map, Value};

use crate::config::AnthropicConfig;
use crate::llm::{
    default_message, parse_first_json_value, FeedbackJsonParser, LlmPromptContextRenderer,
};
use crate::openai::http::{self, join_url, HttpReply, TransportError};

const MESSAGES_PATH: &str = "/v1/messages";
/// SDK 3.3.0(`PrepareRequest.kt`)이 모든 요청에 넣는 기본 헤더 값.
const ANTHROPIC_VERSION: &str = "2026-10-21";
/// `MessageCreateParams.addBeta(AnthropicBeta.SERVER_SIDE_FALLBACK_2026_07_01)`의 헤더 값.
const SERVER_SIDE_FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";
const STRUCTURED_OUTPUT_FAILED: &str = "structured_output_conversion_failed";
/// SDK `Timeout.connect()` 기본값(1분). `.timeout(Duration)`은 연결 제한을 바꾸지 않는다.
const SDK_CONNECT_TIMEOUT: Duration = Duration::from_secs(60);
/// 재시도 대기 기본값과 상한(`RetryingHttpClient.getRetryBackoffDuration`).
const BACKOFF_BASE_SECONDS: f64 = 0.5;
const BACKOFF_MAX_SECONDS: f64 = 8.0;
const MIN_MAX_TOKENS: u64 = 1024;
const EFFORTS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];
/// 서버가 거절 시 대체 모델로 돌려 주는 기능을 지원하는 모델(Haiku에는 없다).
const SERVER_FALLBACK_MODELS: [&str; 3] =
    ["claude-opus-5-5", "claude-sonnet-5-5", "claude-fable-5-2"];

/// 재시도 사이에 잠드는 함수. 시험에서는 실제로 기다리지 않는 것으로 바꾼다.
pub type Sleeper = Arc<dyn Fn(Duration) + Send + Sync>;

/// Anthropic SDK와 같은 연결 제한(60초)만 가진 블로킹 클라이언트.
/// 요청 전체 제한(`timeout-seconds`)은 요청마다 건다.
pub fn build_anthropic_http_client() -> AppResult<Client> {
    http::build_http_client_with_limits(SDK_CONNECT_TIMEOUT, None)
}

/// Claude Messages API로 피드백을 만드는 `LlmClient`.
/// OpenAI Responses와 달리 대화를 서버에 두지 않는다: 호출마다 시스템 프롬프트 전체를 보내고
/// 세션 기억은 사용자 프롬프트에 합쳐 보낸다. 응답 모양은 구조화 출력으로 강제한다.
pub struct AnthropicLlmClient {
    http: Client,
    base_url: String,
    max_tokens: u64,
    effort: String,
    request_timeout: Duration,
    max_retries: u32,
    sleeper: Sleeper,
    #[cfg(test)]
    captured_logs: std::sync::Mutex<Vec<String>>,
}

impl AnthropicLlmClient {
    /// `http`는 `build_anthropic_http_client()`로 만든 것을 넘긴다.
    pub fn new(http: Client, config: &AnthropicConfig) -> Self {
        Self::with_sleeper(http, config, Arc::new(thread::sleep))
    }

    pub fn with_sleeper(http: Client, config: &AnthropicConfig, sleeper: Sleeper) -> Self {
        Self {
            http,
            base_url: java_trim(&config.base_url)
                .trim_end_matches('/')
                .to_string(),
            max_tokens: config.max_tokens.max(MIN_MAX_TOKENS),
            effort: normalize_effort(&config.effort),
            request_timeout: Duration::from_secs(config.timeout_seconds.max(1)),
            max_retries: config.max_retries,
            sleeper,
            #[cfg(test)]
            captured_logs: std::sync::Mutex::new(Vec::new()),
        }
    }

    fn build_body(&self, model: &str, system_prompt: &str, prompt: &str) -> Value {
        let mut body = Map::new();
        body.insert("model".into(), json!(model));
        body.insert("max_tokens".into(), json!(self.max_tokens));
        body.insert(
            "messages".into(),
            json!([{"role": "user", "content": prompt}]),
        );
        body.insert(
            "output_config".into(),
            json!({
                "effort": self.effort,
                "format": {"type": "json_schema", "schema": feedback_schema()}
            }),
        );
        if !java_is_blank(system_prompt) {
            // 코칭 프롬프트는 한 턴의 여러 호출에서 되풀이된다. 모델의 최소 캐시 크기보다
            // 짧은 앞부분은 캐시되지 않을 뿐이다.
            body.insert(
                "system".into(),
                json!([{
                    "type": "text",
                    "text": system_prompt,
                    "cache_control": {"type": "ephemeral"}
                }]),
            );
        }
        if supports_server_fallback(model) {
            body.insert("fallbacks".into(), json!("default"));
        }
        Value::Object(body)
    }

    /// 요청을 보낸다. 재시도는 SDK의 `RetryingHttpClient`와 같다.
    fn send(&self, api_key: &str, model: &str, body: &Value) -> AppResult<Value> {
        if java_is_blank(api_key) {
            return Err(AppError::InvalidArgument(
                "Anthropic API key is required".to_string(),
            ));
        }
        let payload = body.to_string();
        let url = join_url(&self.base_url, MESSAGES_PATH);
        let mut retries: u32 = 0;
        loop {
            let outcome = self.attempt(&url, api_key, model, &payload);
            let wait = match &outcome {
                Ok(reply) => {
                    // 성공 응답도 `x-should-retry: true`가 있으면 SDK는 다시 보낸다.
                    retries += 1;
                    if retries > self.max_retries || !should_retry_status(reply) {
                        return if reply.is_success() {
                            parse_message(&reply.body)
                        } else {
                            Err(describe_failure(reply))
                        };
                    }
                    retry_delay(&reply.headers, retries)
                }
                Err(_) => {
                    retries += 1;
                    if retries > self.max_retries {
                        return Err(failure("could not reach the Anthropic API"));
                    }
                    backoff_delay(retries)
                }
            };
            (self.sleeper)(wait);
        }
    }

    fn attempt(
        &self,
        url: &str,
        api_key: &str,
        model: &str,
        payload: &str,
    ) -> Result<HttpReply, TransportError> {
        let mut request = self
            .http
            .post(url)
            .timeout(self.request_timeout)
            .header("Accept", "application/json")
            .header("anthropic-version", ANTHROPIC_VERSION)
            .bearer_auth(api_key)
            .header("Content-Type", "application/json")
            .body(payload.to_string());
        if supports_server_fallback(model) {
            request = request.header("anthropic-beta", SERVER_SIDE_FALLBACK_BETA);
        }
        http::send(request, None)
    }

    fn to_result(&self, model: &str, message: &ParsedMessage) -> LlmGenerateResult {
        let parsed = parse_first_json_value(&FeedbackJsonParser::extract_json(&message.text));
        match parsed {
            Ok(root) => {
                let feedback = FeedbackJsonParser::parse(&root);
                let summary = feedback.summary().to_string();
                LlmGenerateResult::new(
                    Some(feedback),
                    Some(Vec::new()),
                    Some(LlmConversationState::empty()),
                    Some(&summary),
                )
            }
            Err(reason) => {
                // 구조화 출력은 스키마를 보장하므로 응답이 잘렸을 때만 여기에 온다.
                tracing::warn!(
                    "anthropic.llm.structured_output failure model={model} stopReason={} reason={}",
                    message.stop_reason.as_deref().unwrap_or("unknown"),
                    default_message(&reason)
                );
                LlmGenerateResult::new(
                    Some(Feedback::of_points("", &[], &[], "", &[])),
                    Some(vec![STRUCTURED_OUTPUT_FAILED.to_string()]),
                    Some(LlmConversationState::empty()),
                    Some(""),
                )
            }
        }
    }

    fn log_usage(&self, model: &str, message: &ParsedMessage) {
        if !cfg!(test) && !tracing::enabled!(tracing::Level::DEBUG) {
            return;
        }
        self.emit(format!(
            "anthropic.llm.response model={model} servedBy={} stopReason={} inputTokens={} outputTokens={} cacheReadTokens={}",
            message.served_by,
            message.stop_reason.as_deref().unwrap_or("unknown"),
            message.input_tokens,
            message.output_tokens,
            message.cache_read_tokens
        ));
    }

    fn emit(&self, line: String) {
        tracing::debug!("{line}");
        #[cfg(test)]
        self.captured_logs.lock().unwrap().push(line);
    }
}

impl LlmClient for AnthropicLlmClient {
    fn provider(&self) -> String {
        FeedbackModelPolicy::ANTHROPIC_PROVIDER.to_string()
    }

    fn is_stateless(&self) -> bool {
        true
    }

    fn bootstrap(
        &self,
        _api_key: &str,
        _model: &str,
        _system_prompt: &str,
        _conversation_state: &LlmConversationState,
    ) -> AppResult<LlmConversationState> {
        // 미리 해 둘 일이 없다: 시스템 프롬프트는 generate마다 보낸다.
        Ok(LlmConversationState::empty())
    }

    fn generate(
        &self,
        api_key: &str,
        model: &str,
        system_prompt: &str,
        user_prompt: &str,
        _conversation_state: &LlmConversationState,
        prompt_context: &LlmPromptContext,
    ) -> AppResult<LlmGenerateResult> {
        let resolved_model = resolve_model(model)?;
        let prompt = LlmPromptContextRenderer::merge(user_prompt, prompt_context);
        let body = self.build_body(&resolved_model, system_prompt, &prompt);
        let root = self.send(api_key, &resolved_model, &body)?;
        let message = ParsedMessage::from_json(&root)?;
        self.log_usage(&resolved_model, &message);
        if message.stop_reason.as_deref() == Some("refusal") {
            return Err(AppError::InvalidState(format!(
                "Claude declined to answer this turn (category: {})",
                message.refusal_category
            )));
        }
        Ok(self.to_result(&resolved_model, &message))
    }
}

/// 응답에서 읽는 값만 뽑아 둔 것(Java `BetaMessage`).
struct ParsedMessage {
    text: String,
    stop_reason: Option<String>,
    refusal_category: String,
    served_by: String,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
}

impl ParsedMessage {
    fn from_json(root: &Value) -> AppResult<Self> {
        let Some(blocks) = root.get("content").and_then(Value::as_array) else {
            return Err(AppError::Internal(
                "Anthropic response has no content".to_string(),
            ));
        };
        let text = blocks
            .iter()
            .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|block| block.get("text").and_then(Value::as_str))
            .collect::<String>();
        let usage = root.get("usage");
        let count = |name: &str| {
            usage
                .and_then(|usage| usage.get(name))
                .and_then(Value::as_u64)
                .unwrap_or(0)
        };
        Ok(Self {
            text,
            stop_reason: root
                .get("stop_reason")
                .and_then(Value::as_str)
                .map(str::to_string),
            refusal_category: root
                .get("stop_details")
                .and_then(|details| details.get("category"))
                .and_then(Value::as_str)
                .unwrap_or("unspecified")
                .to_string(),
            served_by: root
                .get("model")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string(),
            input_tokens: count("input_tokens"),
            output_tokens: count("output_tokens"),
            cache_read_tokens: count("cache_read_input_tokens"),
        })
    }
}

fn parse_message(body: &[u8]) -> AppResult<Value> {
    serde_json::from_slice::<Value>(body)
        .map_err(|_| AppError::Internal("Anthropic response could not be parsed".to_string()))
}

fn failure(reason: &str) -> AppError {
    AppError::InvalidState(format!("Claude request failed: {reason}"))
}

/// `AnthropicClientFactory`/`AnthropicLlmClient.describe`와 같은 문장.
fn describe_failure(reply: &HttpReply) -> AppError {
    let reason = match reply.status {
        401 => "the Anthropic API key was rejected".to_string(),
        403 => "this API key cannot use the selected Claude model".to_string(),
        404 => "the selected Claude model was not found".to_string(),
        429 => "rate limit reached, try again shortly".to_string(),
        status => format!(
            "HTTP {status} {}",
            default_message(&service_exception_message(reply))
        ),
    };
    failure(&reason)
}

/// SDK 예외 메시지: `"<상태>: <본문 JSON>"`. 본문이 비면 `Unknown`, JSON이 아니면 글을 JSON 문자열로 감싼다.
fn service_exception_message(reply: &HttpReply) -> String {
    let body = if reply.body.is_empty() {
        "Unknown".to_string()
    } else {
        match serde_json::from_slice::<Value>(&reply.body) {
            Ok(value) => value.to_string(),
            Err(_) => Value::String(reply.text()).to_string(),
        }
    };
    format!("{}: {body}", reply.status)
}

/// `x-should-retry`가 우선하고, 없으면 408, 409, 429, 5xx.
fn should_retry_status(reply: &HttpReply) -> bool {
    match reply
        .headers
        .get("x-should-retry")
        .and_then(|value| value.to_str().ok())
    {
        Some("true") => true,
        Some("false") => false,
        _ => matches!(reply.status, 408 | 409 | 429) || reply.status >= 500,
    }
}

/// 서버가 `Retry-After-Ms`/`Retry-After`로 정한 시간이 있으면 그만큼, 아니면 지수 대기.
fn retry_delay(headers: &HeaderMap, retries: u32) -> Duration {
    retry_after(headers, chrono::Utc::now()).unwrap_or_else(|| backoff_delay(retries))
}

fn retry_after(headers: &HeaderMap, now: chrono::DateTime<chrono::Utc>) -> Option<Duration> {
    let first = |name: &str| {
        headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string)
    };
    let nanos: Option<f64> = match first("retry-after-ms").and_then(|v| parse_float(&v)) {
        Some(millis) => Some(millis * 1_000_000.0),
        None => first("retry-after").and_then(|value| match parse_float(&value) {
            Some(seconds) => Some(seconds * 1_000_000_000.0),
            None => chrono::DateTime::parse_from_rfc2822(&value)
                .ok()
                .and_then(|date| (date.with_timezone(&chrono::Utc) - now).num_nanoseconds())
                .map(|nanos| nanos as f64),
        }),
    };
    // 양수가 아닌 값(지난 날짜 등)은 무시하고 지수 대기를 쓴다.
    nanos
        .filter(|nanos| nanos.is_finite() && *nanos > 0.0)
        .and_then(|nanos| Duration::try_from_secs_f64(nanos / 1_000_000_000.0).ok())
}

fn parse_float(value: &str) -> Option<f64> {
    java_trim(value).parse::<f64>().ok()
}

/// `min(0.5 * 2^(retries-1), 8)`초에 0.75~1.0배 흔들림을 곱한다. `retries`는 1부터.
fn backoff_delay(retries: u32) -> Duration {
    backoff_with_jitter(retries, random_unit())
}

fn backoff_with_jitter(retries: u32, unit_random: f64) -> Duration {
    let exponent = i32::try_from(retries.saturating_sub(1)).unwrap_or(i32::MAX);
    let seconds = (BACKOFF_BASE_SECONDS * 2f64.powi(exponent)).min(BACKOFF_MAX_SECONDS);
    Duration::from_secs_f64(seconds * (1.0 - 0.25 * unit_random))
}

/// 0 이상 1 미만의 값. 흔들림에만 쓰므로 표준 라이브러리의 무작위 해시 시드로 충분하다.
fn random_unit() -> f64 {
    let mut hasher = RandomState::new().build_hasher();
    hasher.write_u8(0);
    (hasher.finish() >> 11) as f64 / (1u64 << 53) as f64
}

fn supports_server_fallback(model: &str) -> bool {
    SERVER_FALLBACK_MODELS.contains(&model)
}

fn resolve_model(model: &str) -> AppResult<String> {
    let candidate = java_trim(model).to_lowercase();
    if java_is_blank(&candidate) {
        return Ok(FeedbackModelPolicy::default_model(Some(
            FeedbackModelPolicy::ANTHROPIC_PROVIDER,
        ))?
        .to_string());
    }
    Ok(candidate)
}

fn normalize_effort(effort: &str) -> String {
    let candidate = java_trim(effort).to_lowercase();
    if EFFORTS.contains(&candidate.as_str()) {
        candidate
    } else {
        "low".to_string()
    }
}

/// 구조화 출력용 JSON 스키마(Java `feedbackSchema()`).
fn feedback_schema() -> Value {
    let text = json!({"type": "string"});
    object_schema(&[
        ("summary", text.clone()),
        (
            "corrections",
            pair_group(&["grammar", "expression", "logic"], "issue", "fix"),
        ),
        (
            "recommendations",
            pair_group(&["filler", "adjective", "adverb"], "term", "usage"),
        ),
        ("exampleAnswer", text.clone()),
        ("rulebookEvidence", json!({"type": "array", "items": text})),
    ])
}

fn pair_group(names: &[&str], first: &str, second: &str) -> Value {
    let pair = object_schema(&[
        (first, json!({"type": "string"})),
        (second, json!({"type": "string"})),
    ]);
    let members: Vec<(&str, Value)> = names.iter().map(|name| (*name, pair.clone())).collect();
    object_schema(&members)
}

/// 모든 속성이 필수이고 다른 속성은 허용하지 않는 객체 스키마. `required`는 넣은 순서다.
fn object_schema(properties: &[(&str, Value)]) -> Value {
    let mut map = Map::new();
    for (name, schema) in properties {
        map.insert((*name).to_string(), schema.clone());
    }
    json!({
        "type": "object",
        "properties": map,
        "required": properties.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
        "additionalProperties": false
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::Mutex;
    use std::time::Instant;

    const FEEDBACK_JSON: &str = r#"{"summary":"Clear answer with one tense slip.",
 "corrections":{
   "grammar":{"issue":"I go there yesterday","fix":"I went there yesterday"},
   "expression":{"issue":"very good","fix":"really enjoyable"},
   "logic":{"issue":"no reason given","fix":"add why you liked it"}},
 "recommendations":{
   "filler":{"term":"Well","usage":"Open your answer with it."},
   "adjective":{"term":"peaceful","usage":"Describe the park."},
   "adverb":{"term":"definitely","usage":"Stress your opinion."}},
 "exampleAnswer":"Well, I went to a peaceful park yesterday.",
 "rulebookEvidence":["[travel.md] use past tense"]}"#;

    struct Recorded {
        request_line: String,
        headers: Vec<(String, String)>,
        body: String,
    }

    impl Recorded {
        fn header(&self, name: &str) -> Option<&str> {
            self.headers
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.as_str())
        }

        fn json(&self) -> Value {
            serde_json::from_str(&self.body).unwrap()
        }
    }

    enum Step {
        Reply {
            status: u16,
            headers: Vec<(&'static str, String)>,
            body: String,
        },
        /// 응답 없이 연결을 끊는다.
        Drop,
        /// 요청을 다 읽고 `delay` 뒤에 응답한다.
        Slow(Duration, String),
    }

    fn reply(status: u16, body: impl Into<String>) -> Step {
        Step::Reply {
            status,
            headers: Vec::new(),
            body: body.into(),
        }
    }

    fn reply_with(status: u16, headers: Vec<(&'static str, &str)>, body: &str) -> Step {
        Step::Reply {
            status,
            headers: headers
                .into_iter()
                .map(|(name, value)| (name, value.to_string()))
                .collect(),
            body: body.to_string(),
        }
    }

    struct TestServer {
        base_url: String,
        requests: Arc<Mutex<Vec<Recorded>>>,
    }

    impl TestServer {
        fn request_count(&self) -> usize {
            self.requests.lock().unwrap().len()
        }

        fn take(&self, index: usize) -> Recorded {
            let mut requests = self.requests.lock().unwrap();
            requests.remove(index)
        }
    }

    fn read_request(stream: &mut TcpStream) -> Option<Recorded> {
        let mut received = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            let read = stream.read(&mut chunk).ok()?;
            if read == 0 {
                return None;
            }
            received.extend_from_slice(&chunk[..read]);
            let text = String::from_utf8_lossy(&received).into_owned();
            let Some(end) = text.find("\r\n\r\n") else {
                continue;
            };
            let mut lines = text[..end].lines();
            let request_line = lines.next().unwrap_or("").to_string();
            let headers: Vec<(String, String)> = lines
                .filter_map(|line| line.split_once(':'))
                .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_string()))
                .collect();
            let length = headers
                .iter()
                .find(|(name, _)| name == "content-length")
                .and_then(|(_, value)| value.parse::<usize>().ok())
                .unwrap_or(0);
            if received.len() >= end + 4 + length {
                let body =
                    String::from_utf8_lossy(&received[end + 4..end + 4 + length]).into_owned();
                return Some(Recorded {
                    request_line,
                    headers,
                    body,
                });
            }
        }
    }

    fn write_response(stream: &mut TcpStream, status: u16, headers: &[(&str, String)], body: &str) {
        let mut head = format!(
            "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n",
            body.len()
        );
        for (name, value) in headers {
            head.push_str(&format!("{name}: {value}\r\n"));
        }
        head.push_str("\r\n");
        let _ = stream.write_all(head.as_bytes());
        let _ = stream.write_all(body.as_bytes());
    }

    /// `script`를 요청 순서대로 돌려주고, 끝나면 마지막 단계를 되풀이한다. 요청은 모두 기록한다.
    fn serve(script: Vec<Step>) -> TestServer {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let requests: Arc<Mutex<Vec<Recorded>>> = Arc::new(Mutex::new(Vec::new()));
        let recorded = requests.clone();
        thread::spawn(move || {
            let mut index = 0usize;
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let Some(request) = read_request(&mut stream) else {
                    continue;
                };
                recorded.lock().unwrap().push(request);
                match &script[index.min(script.len() - 1)] {
                    Step::Reply {
                        status,
                        headers,
                        body,
                    } => write_response(&mut stream, *status, headers, body),
                    Step::Drop => {}
                    Step::Slow(delay, body) => {
                        thread::sleep(*delay);
                        write_response(&mut stream, 200, &[], body);
                    }
                }
                index += 1;
            }
        });
        TestServer {
            base_url: format!("http://{address}"),
            requests,
        }
    }

    fn message_body(model: &str, stop_reason: &str, text: &str) -> String {
        json!({
            "id": "msg_test",
            "type": "message",
            "role": "assistant",
            "model": model,
            "content": [{"type": "text", "text": text}],
            "stop_reason": stop_reason,
            "usage": {"input_tokens": 120, "output_tokens": 80}
        })
        .to_string()
    }

    fn refusal_body() -> String {
        json!({
            "id": "msg_refused",
            "type": "message",
            "role": "assistant",
            "model": "claude-haiku-5-5",
            "content": [],
            "stop_reason": "refusal",
            "stop_details": {"type": "refusal", "category": "cyber", "explanation": "declined"},
            "usage": {"input_tokens": 10, "output_tokens": 0}
        })
        .to_string()
    }

    fn config(base_url: &str, effort: &str, max_retries: u32) -> AnthropicConfig {
        AnthropicConfig {
            base_url: base_url.to_string(),
            max_tokens: 16_000,
            effort: effort.to_string(),
            timeout_seconds: 120,
            max_retries,
        }
    }

    fn client_with(server: &TestServer, effort: &str, max_retries: u32) -> AnthropicLlmClient {
        AnthropicLlmClient::new(
            build_anthropic_http_client().unwrap(),
            &config(&format!("{}/", server.base_url), effort, max_retries),
        )
    }

    fn client(server: &TestServer, effort: &str) -> AnthropicLlmClient {
        client_with(server, effort, 0)
    }

    type Waits = Arc<Mutex<Vec<Duration>>>;

    fn recording_client(server: &TestServer, max_retries: u32) -> (AnthropicLlmClient, Waits) {
        let waits: Waits = Arc::new(Mutex::new(Vec::new()));
        let recorded = waits.clone();
        let client = AnthropicLlmClient::with_sleeper(
            build_anthropic_http_client().unwrap(),
            &config(&server.base_url, "low", max_retries),
            Arc::new(move |wait| recorded.lock().unwrap().push(wait)),
        );
        (client, waits)
    }

    fn generate(
        client: &AnthropicLlmClient,
        key: &str,
        model: &str,
        prompt_context: &LlmPromptContext,
    ) -> AppResult<LlmGenerateResult> {
        client.generate(
            key,
            model,
            "system",
            "prompt",
            &LlmConversationState::empty(),
            prompt_context,
        )
    }

    fn generate_simple(client: &AnthropicLlmClient, model: &str) -> AppResult<LlmGenerateResult> {
        generate(client, "sk-ant-key", model, &LlmPromptContext::empty())
    }

    fn invalid_state(result: AppResult<LlmGenerateResult>) -> String {
        match result.expect_err("expected an error") {
            AppError::InvalidState(message) => message,
            other => panic!("expected InvalidState, got {other:?}"),
        }
    }

    #[test]
    fn provider_is_anthropic_and_stateless() {
        let server = serve(vec![reply(200, "{}")]);
        let client = client(&server, "low");

        assert_eq!("anthropic", client.provider());
        assert!(client.is_stateless());
    }

    #[test]
    fn bootstrap_makes_no_request() {
        let server = serve(vec![reply(200, "{}")]);
        let client = client(&server, "low");

        let state = client
            .bootstrap(
                "sk-ant-key",
                "claude-opus-5-5",
                "system",
                &LlmConversationState::empty(),
            )
            .unwrap();

        assert!(!state.has_conversation_id());
        assert_eq!(0, server.request_count());
    }

    #[test]
    fn generate_sends_structured_output_request_and_parses_feedback() {
        let server = serve(vec![reply(
            200,
            message_body("claude-opus-5-5", "end_turn", FEEDBACK_JSON),
        )]);
        let client = client(&server, "medium");

        let result = client
            .generate(
                "sk-ant-key",
                "claude-opus-5-5",
                "coach system prompt",
                "Question: park\nAnswer: I go there yesterday",
                &LlmConversationState::empty(),
                &LlmPromptContext::empty(),
            )
            .unwrap();

        assert_eq!(
            "Clear answer with one tense slip.",
            result.feedback.summary()
        );
        assert_eq!(
            "I went there yesterday",
            result.feedback.corrections().grammar().fix()
        );
        assert_eq!(
            "peaceful",
            result.feedback.recommendations().adjective().term()
        );
        assert!(result.schema_fallback_reasons.is_empty());
        assert!(!result.conversation_state.has_conversation_id());

        let request = server.take(0);
        assert_eq!("POST /v1/messages HTTP/1.1", request.request_line);
        assert_eq!(Some("Bearer sk-ant-key"), request.header("authorization"));
        assert_eq!(Some("application/json"), request.header("accept"));
        assert_eq!(Some("application/json"), request.header("content-type"));
        assert_eq!(Some("2026-10-21"), request.header("anthropic-version"));
        assert_eq!(None, request.header("x-api-key"));
        let payload = request.json();
        assert_eq!("claude-opus-5-5", payload["model"]);
        assert_eq!(16000, payload["max_tokens"]);
        assert_eq!("coach system prompt", payload["system"][0]["text"]);
        assert_eq!("text", payload["system"][0]["type"]);
        assert_eq!("ephemeral", payload["system"][0]["cache_control"]["type"]);
        assert_eq!("user", payload["messages"][0]["role"]);
        assert_eq!(
            "Question: park\nAnswer: I go there yesterday",
            payload["messages"][0]["content"]
        );
        assert_eq!("medium", payload["output_config"]["effort"]);
        let format = &payload["output_config"]["format"];
        assert_eq!("json_schema", format["type"]);
        assert_eq!("object", format["schema"]["type"]);
        assert_eq!(false, format["schema"]["additionalProperties"]);
        assert_eq!(5, format["schema"]["required"].as_array().unwrap().len());
        assert_eq!(
            json!([
                "summary",
                "corrections",
                "recommendations",
                "exampleAnswer",
                "rulebookEvidence"
            ]),
            format["schema"]["required"]
        );
        assert!(format["schema"]["properties"]["corrections"]["properties"]
            .get("grammar")
            .is_some());
        assert_eq!(
            json!(["issue", "fix"]),
            format["schema"]["properties"]["corrections"]["properties"]["logic"]["required"]
        );
        assert_eq!(
            json!(["term", "usage"]),
            format["schema"]["properties"]["recommendations"]["properties"]["filler"]["required"]
        );
        assert!(payload.get("thinking").is_none());
        assert!(payload.get("temperature").is_none());
    }

    #[test]
    fn generate_enables_server_side_refusal_fallback_for_models_that_support_it() {
        let server = serve(vec![reply(
            200,
            message_body("claude-opus-5-5", "end_turn", FEEDBACK_JSON),
        )]);
        let client = client(&server, "low");

        generate_simple(&client, "claude-opus-5-5").unwrap();

        let request = server.take(0);
        assert!(request
            .header("anthropic-beta")
            .unwrap()
            .contains("server-side-fallback-2026-07-01"));
        assert_eq!("default", request.json()["fallbacks"]);
    }

    #[test]
    fn generate_omits_fallback_for_haiku() {
        let server = serve(vec![reply(
            200,
            message_body("claude-haiku-5-5", "end_turn", FEEDBACK_JSON),
        )]);
        let client = client(&server, "low");

        generate_simple(&client, "claude-haiku-5-5").unwrap();

        let request = server.take(0);
        assert!(request.json().get("fallbacks").is_none());
        assert_eq!(None, request.header("anthropic-beta"));
    }

    #[test]
    fn generate_merges_session_memory_into_user_prompt() {
        let server = serve(vec![reply(
            200,
            message_body("claude-sonnet-5-5", "end_turn", FEEDBACK_JSON),
        )]);
        let client = client(&server, "low");
        let context = LlmPromptContext::new(
            "learner mixes tenses",
            vec![omypic_domain::session::TurnRecord::new(
                "Q1",
                "A1",
                "summary-1",
            )],
            vec![omypic_domain::session::RecommendationRecord::new(
                "Well",
                "peaceful",
                "definitely",
            )],
        );

        client
            .generate(
                "sk-ant-key",
                "claude-sonnet-5-5",
                "system",
                "current turn",
                &LlmConversationState::empty(),
                &context,
            )
            .unwrap();

        let payload = server.take(0).json();
        let user_text = payload["messages"][0]["content"].to_string();
        assert!(user_text.contains("learner mixes tenses"));
        assert!(user_text.contains("Recent turns"));
        assert!(user_text.contains("current turn"));
    }

    #[test]
    fn generate_throws_when_claude_declines() {
        let server = serve(vec![reply(200, refusal_body())]);
        let client = client(&server, "low");

        let message = invalid_state(generate_simple(&client, "claude-haiku-5-5"));

        assert!(message.contains("declined"), "{message}");
        assert_eq!(
            "Claude declined to answer this turn (category: cyber)",
            message
        );
    }

    #[test]
    fn generate_reports_schema_fallback_when_reply_is_cut_off() {
        let server = serve(vec![reply(
            200,
            message_body("claude-haiku-5-5", "max_tokens", "{\"summary\":\"cut"),
        )]);
        let client = client(&server, "low");

        let result = generate_simple(&client, "claude-haiku-5-5").unwrap();

        assert_eq!(
            vec!["structured_output_conversion_failed".to_string()],
            result.schema_fallback_reasons
        );
        assert_eq!("", result.feedback.summary());
    }

    #[test]
    fn generate_explains_rejected_api_key() {
        let server = serve(vec![reply(
            401,
            r#"{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"}}"#,
        )]);
        let client = client(&server, "low");

        let error = generate(
            &client,
            "sk-ant-bad",
            "claude-haiku-5-5",
            &LlmPromptContext::empty(),
        );

        assert_eq!(
            "Claude request failed: the Anthropic API key was rejected",
            invalid_state(error)
        );
        assert_eq!(1, server.request_count());
    }

    #[test]
    fn retries_on_529_then_succeeds() {
        let server = serve(vec![
            reply(
                529,
                r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#,
            ),
            reply(
                200,
                message_body("claude-haiku-5-5", "end_turn", FEEDBACK_JSON),
            ),
        ]);
        let (client, waits) = recording_client(&server, 2);

        let result = generate_simple(&client, "claude-haiku-5-5").unwrap();

        assert_eq!(
            "Clear answer with one tense slip.",
            result.feedback.summary()
        );
        assert_eq!(2, server.request_count());
        let waits = waits.lock().unwrap();
        assert_eq!(1, waits.len());
        assert!(waits[0] >= Duration::from_millis(375) && waits[0] <= Duration::from_millis(500));
    }

    #[test]
    fn gives_up_after_max_retries() {
        let server = serve(vec![reply(500, r#"{"error":"boom"}"#)]);
        let (client, waits) = recording_client(&server, 2);

        let message = invalid_state(generate_simple(&client, "claude-haiku-5-5"));

        assert_eq!(3, server.request_count());
        assert_eq!(
            r#"Claude request failed: HTTP 500 500: {"error":"boom"}"#,
            message
        );
        let waits = waits.lock().unwrap();
        assert_eq!(2, waits.len());
        assert!(waits[0] >= Duration::from_millis(375) && waits[0] <= Duration::from_millis(500));
        assert!(waits[1] >= Duration::from_millis(750) && waits[1] <= Duration::from_secs(1));
    }

    #[test]
    fn max_retries_zero_sends_a_single_request() {
        let server = serve(vec![reply(503, "{}")]);
        let (client, waits) = recording_client(&server, 0);

        assert!(generate_simple(&client, "claude-haiku-5-5").is_err());

        assert_eq!(1, server.request_count());
        assert!(waits.lock().unwrap().is_empty());
    }

    #[test]
    fn retries_408_409_and_429_but_not_other_client_errors() {
        for status in [408u16, 409, 429] {
            let server = serve(vec![
                reply(status, "{}"),
                reply(
                    200,
                    message_body("claude-haiku-5-5", "end_turn", FEEDBACK_JSON),
                ),
            ]);
            let (client, _) = recording_client(&server, 2);
            generate_simple(&client, "claude-haiku-5-5").unwrap();
            assert_eq!(2, server.request_count(), "status {status}");
        }
        for status in [400u16, 401, 403, 404, 422] {
            let server = serve(vec![reply(status, "{}")]);
            let (client, _) = recording_client(&server, 2);
            assert!(generate_simple(&client, "claude-haiku-5-5").is_err());
            assert_eq!(1, server.request_count(), "status {status}");
        }
    }

    #[test]
    fn x_should_retry_header_overrides_the_status() {
        let server = serve(vec![reply_with(
            500,
            vec![("x-should-retry", "false")],
            "{}",
        )]);
        let (client, _) = recording_client(&server, 2);
        assert!(generate_simple(&client, "claude-haiku-5-5").is_err());
        assert_eq!(1, server.request_count());

        let server = serve(vec![
            reply_with(400, vec![("x-should-retry", "true")], "{}"),
            reply(
                200,
                message_body("claude-haiku-5-5", "end_turn", FEEDBACK_JSON),
            ),
        ]);
        let (client, _) = recording_client(&server, 2);
        generate_simple(&client, "claude-haiku-5-5").unwrap();
        assert_eq!(2, server.request_count());
    }

    #[test]
    fn retry_after_headers_set_the_wait() {
        let server = serve(vec![
            reply_with(
                429,
                vec![("retry-after-ms", "1500"), ("retry-after", "9")],
                "{}",
            ),
            reply_with(429, vec![("retry-after", "3")], "{}"),
            reply_with(429, vec![("retry-after", "-1")], "{}"),
            reply(
                200,
                message_body("claude-haiku-5-5", "end_turn", FEEDBACK_JSON),
            ),
        ]);
        let (client, waits) = recording_client(&server, 3);

        generate_simple(&client, "claude-haiku-5-5").unwrap();

        let waits = waits.lock().unwrap();
        assert_eq!(Duration::from_millis(1500), waits[0]);
        assert_eq!(Duration::from_secs(3), waits[1]);
        // 양수가 아닌 값은 무시하고 세 번째 재시도의 지수 대기(0.5 * 2^2 = 2초, 흔들림 0.75~1)를 쓴다.
        assert!(waits[2] >= Duration::from_millis(1500) && waits[2] <= Duration::from_secs(2));
    }

    #[test]
    fn retry_after_accepts_http_dates_and_rejects_the_past() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-11-19T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let mut headers = HeaderMap::new();
        headers.insert(
            "retry-after",
            "Thu, 19 Nov 2026 00:00:07 GMT".parse().unwrap(),
        );
        assert_eq!(Some(Duration::from_secs(7)), retry_after(&headers, now));

        headers.insert(
            "retry-after",
            "Wed, 18 Nov 2026 23:59:00 GMT".parse().unwrap(),
        );
        assert_eq!(None, retry_after(&headers, now));

        headers.insert("retry-after", "soon".parse().unwrap());
        assert_eq!(None, retry_after(&headers, now));

        headers.insert("retry-after-ms", "250".parse().unwrap());
        assert_eq!(Some(Duration::from_millis(250)), retry_after(&headers, now));
    }

    #[test]
    fn backoff_doubles_up_to_eight_seconds_with_downward_jitter() {
        assert_eq!(Duration::from_millis(500), backoff_with_jitter(1, 0.0));
        assert_eq!(Duration::from_millis(375), backoff_with_jitter(1, 1.0));
        assert_eq!(Duration::from_secs(1), backoff_with_jitter(2, 0.0));
        assert_eq!(Duration::from_secs(4), backoff_with_jitter(4, 0.0));
        assert_eq!(Duration::from_secs(8), backoff_with_jitter(5, 0.0));
        assert_eq!(Duration::from_secs(8), backoff_with_jitter(40, 0.0));
        for _ in 0..50 {
            let wait = backoff_delay(3);
            assert!(wait >= Duration::from_millis(1500) && wait <= Duration::from_secs(2));
        }
    }

    #[test]
    fn connection_failures_are_retried_then_reported() {
        let server = serve(vec![
            Step::Drop,
            reply(
                200,
                message_body("claude-haiku-5-5", "end_turn", FEEDBACK_JSON),
            ),
        ]);
        let (client, _) = recording_client(&server, 2);
        generate_simple(&client, "claude-haiku-5-5").unwrap();
        assert_eq!(2, server.request_count());

        let server = serve(vec![Step::Drop]);
        let (client, waits) = recording_client(&server, 1);
        let message = invalid_state(generate_simple(&client, "claude-haiku-5-5"));
        assert_eq!(
            "Claude request failed: could not reach the Anthropic API",
            message
        );
        assert_eq!(2, server.request_count());
        assert_eq!(1, waits.lock().unwrap().len());
    }

    #[test]
    fn unreachable_server_is_reported_without_the_key() {
        let port = {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            listener.local_addr().unwrap().port()
        };
        let client = AnthropicLlmClient::with_sleeper(
            build_anthropic_http_client().unwrap(),
            &config(&format!("http://127.0.0.1:{port}"), "low", 1),
            Arc::new(|_| {}),
        );

        let message = invalid_state(generate_simple(&client, "claude-haiku-5-5"));

        assert_eq!(
            "Claude request failed: could not reach the Anthropic API",
            message
        );
        assert!(!message.contains("sk-ant-key"));
    }

    #[test]
    fn describes_other_http_failures() {
        let cases: [(u16, &str, &str); 6] = [
            (
                403,
                "{}",
                "Claude request failed: this API key cannot use the selected Claude model",
            ),
            (
                404,
                "{}",
                "Claude request failed: the selected Claude model was not found",
            ),
            (
                429,
                "{}",
                "Claude request failed: rate limit reached, try again shortly",
            ),
            (
                400,
                r#"{"error":{"message":"bad"}}"#,
                r#"Claude request failed: HTTP 400 400: {"error":{"message":"bad"}}"#,
            ),
            (
                502,
                "bad gateway",
                r#"Claude request failed: HTTP 502 502: "bad gateway""#,
            ),
            (503, "", "Claude request failed: HTTP 503 503: Unknown"),
        ];
        for (status, body, expected) in cases {
            let server = serve(vec![reply(status, body)]);
            let client = client(&server, "low");
            assert_eq!(
                expected,
                invalid_state(generate_simple(&client, "claude-haiku-5-5"))
            );
        }
    }

    #[test]
    fn request_timeout_is_a_total_deadline_per_attempt() {
        let server = serve(vec![Step::Slow(
            Duration::from_secs(5),
            message_body("claude-haiku-5-5", "end_turn", FEEDBACK_JSON),
        )]);
        let mut slow = config(&server.base_url, "low", 0);
        slow.timeout_seconds = 1;
        let client = AnthropicLlmClient::new(build_anthropic_http_client().unwrap(), &slow);

        let started = Instant::now();
        let message = invalid_state(generate_simple(&client, "claude-haiku-5-5"));

        assert_eq!(
            "Claude request failed: could not reach the Anthropic API",
            message
        );
        assert!(
            started.elapsed() < Duration::from_secs(4),
            "{:?}",
            started.elapsed()
        );
    }

    #[test]
    fn blank_api_key_is_rejected_before_any_request() {
        let server = serve(vec![reply(200, "{}")]);
        let client = client(&server, "low");

        for key in ["", "   "] {
            let error = generate(&client, key, "claude-haiku-5-5", &LlmPromptContext::empty())
                .err()
                .unwrap();
            let AppError::InvalidArgument(message) = error else {
                panic!("expected InvalidArgument, got {error:?}");
            };
            assert_eq!("Anthropic API key is required", message);
        }
        assert_eq!(0, server.request_count());
    }

    #[test]
    fn blank_model_uses_the_default_and_names_are_lowercased() {
        let server = serve(vec![reply(
            200,
            message_body("claude-haiku-5-5", "end_turn", FEEDBACK_JSON),
        )]);
        let client = client(&server, "low");

        generate_simple(&client, "  ").unwrap();
        generate_simple(&client, " Claude-Haiku-5-5 ").unwrap();

        let default = FeedbackModelPolicy::default_model(Some("anthropic")).unwrap();
        assert_eq!(default, server.take(0).json()["model"]);
        assert_eq!("claude-haiku-5-5", server.take(0).json()["model"]);
    }

    #[test]
    fn effort_and_max_tokens_are_normalized() {
        let server = serve(vec![reply(
            200,
            message_body("claude-haiku-5-5", "end_turn", FEEDBACK_JSON),
        )]);
        for (effort, expected) in [
            (" HIGH ", "high"),
            ("xhigh", "xhigh"),
            ("turbo", "low"),
            ("", "low"),
        ] {
            let mut settings = config(&server.base_url, effort, 0);
            settings.max_tokens = 10;
            let client = AnthropicLlmClient::new(build_anthropic_http_client().unwrap(), &settings);
            generate_simple(&client, "claude-haiku-5-5").unwrap();
            let payload = server.take(0).json();
            assert_eq!(expected, payload["output_config"]["effort"]);
            assert_eq!(1024, payload["max_tokens"]);
        }
    }

    #[test]
    fn blank_system_prompt_omits_the_system_field() {
        let server = serve(vec![reply(
            200,
            message_body("claude-haiku-5-5", "end_turn", FEEDBACK_JSON),
        )]);
        let client = client(&server, "low");

        client
            .generate(
                "sk-ant-key",
                "claude-haiku-5-5",
                "  ",
                "prompt",
                &LlmConversationState::empty(),
                &LlmPromptContext::empty(),
            )
            .unwrap();

        assert!(server.take(0).json().get("system").is_none());
    }

    #[test]
    fn text_blocks_are_joined_and_other_blocks_ignored() {
        let body = json!({
            "id": "msg_test",
            "type": "message",
            "role": "assistant",
            "model": "claude-haiku-5-5",
            "content": [
                {"type": "thinking", "thinking": "hmm", "signature": "s"},
                {"type": "text", "text": "{\"summary\":\"first "},
                {"type": "text", "text": "half\"}"}
            ],
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 1, "output_tokens": 1}
        })
        .to_string();
        let server = serve(vec![reply(200, body)]);
        let client = client(&server, "low");

        let result = generate_simple(&client, "claude-haiku-5-5").unwrap();

        assert_eq!("first half", result.feedback.summary());
        assert!(result.schema_fallback_reasons.is_empty());
    }

    #[test]
    fn refusal_without_category_reports_unspecified() {
        let body = json!({
            "id": "m", "type": "message", "role": "assistant", "model": "claude-haiku-5-5",
            "content": [], "stop_reason": "refusal", "usage": {"input_tokens": 1, "output_tokens": 0}
        })
        .to_string();
        let server = serve(vec![reply(200, body)]);
        let client = client(&server, "low");

        assert_eq!(
            "Claude declined to answer this turn (category: unspecified)",
            invalid_state(generate_simple(&client, "claude-haiku-5-5"))
        );
    }

    #[test]
    fn logged_lines_and_errors_never_contain_the_api_key() {
        let server = serve(vec![
            reply(
                200,
                message_body("claude-haiku-5-5", "end_turn", FEEDBACK_JSON),
            ),
            reply(401, r#"{"error":{"message":"invalid"}}"#),
        ]);
        let client = client(&server, "low");

        generate(
            &client,
            "sk-test-secret",
            "claude-haiku-5-5",
            &LlmPromptContext::empty(),
        )
        .unwrap();
        let message = invalid_state(generate(
            &client,
            "sk-test-secret",
            "claude-haiku-5-5",
            &LlmPromptContext::empty(),
        ));

        assert!(!message.contains("sk-test-secret"));
        let lines = client.captured_logs.lock().unwrap().clone();
        assert_eq!(1, lines.len());
        assert_eq!(
            "anthropic.llm.response model=claude-haiku-5-5 servedBy=claude-haiku-5-5 stopReason=end_turn inputTokens=120 outputTokens=80 cacheReadTokens=0",
            lines[0]
        );
        for line in &lines {
            assert!(!line.contains("sk-test-secret"), "{line}");
            assert!(!line.to_lowercase().contains("authorization"), "{line}");
            assert!(!line.to_lowercase().contains("bearer"), "{line}");
        }
    }

    #[test]
    fn unparsable_success_body_is_an_internal_error() {
        let server = serve(vec![reply(200, "not json")]);
        let client = client(&server, "low");

        let error = generate_simple(&client, "claude-haiku-5-5").err().unwrap();

        assert!(matches!(error, AppError::Internal(_)), "{error:?}");
    }
}
