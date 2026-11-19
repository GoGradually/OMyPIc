use std::sync::OnceLock;

use omypic_application::feedback::port::{LlmClient, LlmGenerateResult};
use omypic_domain::error::{AppError, AppResult};
use omypic_domain::feedback::{
    CorrectionDetail, Corrections, Feedback, RecommendationDetail, Recommendations,
};
use omypic_domain::session::{LlmConversationState, LlmPromptContext};
use omypic_domain::shared::text_utils::{java_is_blank, java_trim};
use reqwest::blocking::Client;
use serde_json::{json, Map, Value};

use super::http::{self, join_url, MAX_BODY_BYTES};
use super::log_formatter::OpenAiLlmLogFormatter;
use super::parameter_policy::OpenAiModelParameterPolicyResolver;
use crate::config::OpenAiConfig;
use crate::llm::{
    as_text, default_message, parse_first_json_value, path, path_index, FeedbackJsonParser,
    LlmPromptContextRenderer,
};

const DEFAULT_CHAT_MODEL: &str = "gpt-4o-mini";
const BOOTSTRAP_INPUT_TEXT: &str =
    "Acknowledge the coaching strategy and wait for the next user answer.";
const RESPONSES_PATH: &str = "/v1/responses";
const CHAT_COMPLETIONS_PATH: &str = "/v1/chat/completions";
const MAX_STRUCTURED_ATTEMPTS: i32 = 2;

/// 구조화 출력 JSON 스키마. `BeanOutputConverter`(Spring AI 1.0.3)가 victools 4.37.0
/// (`DRAFT_2020_12`, `PLAIN_JSON`, Jackson 모듈, `FORBIDDEN_ADDITIONAL_PROPERTIES_BY_DEFAULT`)으로
/// `StructuredFeedbackResponse`에서 만들어 Jackson `DefaultPrettyPrinter`로 찍던 문자열이다.
/// 속성은 이름순(기본 정렬), 두 번 이상 쓰이는 `CorrectionNode`/`RecommendationNode`만 `$defs`로 빼고
/// 한 번 쓰이는 `CorrectionsNode`/`RecommendationsNode`는 인라인이며, `$defs`는 `type`보다 앞에 온다.
const STRUCTURED_SCHEMA: &str = r##"{
  "$schema" : "https://json-schema.org/draft/2020-12/schema",
  "$defs" : {
    "CorrectionNode" : {
      "type" : "object",
      "properties" : {
        "fix" : {
          "type" : "string"
        },
        "issue" : {
          "type" : "string"
        }
      },
      "additionalProperties" : false
    },
    "RecommendationNode" : {
      "type" : "object",
      "properties" : {
        "term" : {
          "type" : "string"
        },
        "usage" : {
          "type" : "string"
        }
      },
      "additionalProperties" : false
    }
  },
  "type" : "object",
  "properties" : {
    "corrections" : {
      "type" : "object",
      "properties" : {
        "expression" : {
          "$ref" : "#/$defs/CorrectionNode"
        },
        "grammar" : {
          "$ref" : "#/$defs/CorrectionNode"
        },
        "logic" : {
          "$ref" : "#/$defs/CorrectionNode"
        }
      },
      "additionalProperties" : false
    },
    "exampleAnswer" : {
      "type" : "string"
    },
    "recommendations" : {
      "type" : "object",
      "properties" : {
        "adjective" : {
          "$ref" : "#/$defs/RecommendationNode"
        },
        "adverb" : {
          "$ref" : "#/$defs/RecommendationNode"
        },
        "filler" : {
          "$ref" : "#/$defs/RecommendationNode"
        }
      },
      "additionalProperties" : false
    },
    "rulebookEvidence" : {
      "type" : "array",
      "items" : {
        "type" : "string"
      }
    },
    "summary" : {
      "type" : "string"
    }
  },
  "additionalProperties" : false
}"##;

/// `BeanOutputConverter.getFormat()`(Spring AI 1.0.3)가 프롬프트 끝에 붙이던 안내문.
fn structured_output_format() -> &'static str {
    static FORMAT: OnceLock<String> = OnceLock::new();
    FORMAT.get_or_init(|| {
        format!(
            "Your response should be in JSON format.\n\
             Do not include any explanations, only provide a RFC8259 compliant JSON response following this format without deviation.\n\
             Do not include markdown code blocks in your response.\n\
             Remove the ```json markdown from the output.\n\
             Here is the JSON Schema instance your output must adhere to:\n\
             ```{STRUCTURED_SCHEMA}```\n"
        )
    })
}

/// OpenAI Responses API(기본) 또는 Chat Completions로 피드백을 만드는 `LlmClient`.
pub struct OpenAiLlmClient {
    http: Client,
    base_url: String,
    responses_enabled: bool,
    log_formatter: OpenAiLlmLogFormatter,
    policy_resolver: OpenAiModelParameterPolicyResolver,
    #[cfg(test)]
    captured_logs: std::sync::Mutex<Vec<String>>,
}

struct ResponseEnvelope {
    content: String,
    conversation_state: LlmConversationState,
}

/// HTTP 요청 단계의 실패. 상태 코드 오류만 "OpenAI request failed" 접두어가 붙는다.
enum RequestFailure {
    Status(String),
    Other(AppError),
}

enum StructuredFailure {
    /// 응답은 받았지만 구조화 출력으로 바꾸지 못했다(재시도, 폴백 대상).
    Output {
        raw: String,
        conversation_state: LlmConversationState,
    },
    Fatal(AppError),
}

impl OpenAiLlmClient {
    pub fn new(http: Client, config: &OpenAiConfig) -> Self {
        Self {
            http,
            base_url: config.base_url.clone(),
            responses_enabled: config.responses_enabled,
            log_formatter: OpenAiLlmLogFormatter::from_config(&config.logging),
            policy_resolver: OpenAiModelParameterPolicyResolver::new(),
            #[cfg(test)]
            captured_logs: std::sync::Mutex::new(Vec::new()),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn structured_generate(
        &self,
        api_key: &str,
        model: &str,
        system_prompt: &str,
        user_prompt: &str,
        conversation_state: &LlmConversationState,
        prompt_context: &LlmPromptContext,
        attempt: i32,
    ) -> Result<LlmGenerateResult, StructuredFailure> {
        let envelope = self
            .request_content(
                api_key,
                model,
                system_prompt,
                user_prompt,
                prompt_context,
                conversation_state,
                attempt,
            )
            .map_err(StructuredFailure::Fatal)?;
        match convert_structured_output(&envelope.content) {
            Ok(response) => {
                let summary = safe(response.summary.as_deref());
                let feedback = Feedback::of(
                    &summary,
                    to_corrections(response.corrections.as_ref()),
                    to_recommendations(response.recommendations.as_ref()),
                    &safe(response.example_answer.as_deref()),
                    &safe_string_list(response.rulebook_evidence.as_deref()),
                );
                Ok(LlmGenerateResult::new(
                    Some(feedback),
                    Some(Vec::new()),
                    Some(envelope.conversation_state),
                    Some(&summary),
                ))
            }
            Err(_) => Err(StructuredFailure::Output {
                raw: envelope.content,
                conversation_state: envelope.conversation_state,
            }),
        }
    }

    fn fallback_from_structured_failure(
        &self,
        raw: &str,
        conversation_state: LlmConversationState,
    ) -> LlmGenerateResult {
        let reasons = vec!["structured_output_conversion_failed".to_string()];
        match parse_first_json_value(&FeedbackJsonParser::extract_json(raw)) {
            Ok(root) => {
                let feedback = FeedbackJsonParser::parse(&root);
                let summary = feedback.summary().to_string();
                LlmGenerateResult::new(
                    Some(feedback),
                    Some(reasons),
                    Some(conversation_state),
                    Some(&summary),
                )
            }
            Err(reason) => {
                tracing::warn!(
                    "openai.llm.fallback_parse failure reason={}",
                    default_message(&reason)
                );
                LlmGenerateResult::new(
                    Some(Feedback::of_points("", &[], &[], "", &[])),
                    Some(vec![
                        "structured_output_conversion_failed".to_string(),
                        "fallback_parse_failed".to_string(),
                    ]),
                    Some(conversation_state),
                    Some(""),
                )
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn request_content(
        &self,
        api_key: &str,
        model: &str,
        system_prompt: &str,
        user_prompt: &str,
        prompt_context: &LlmPromptContext,
        conversation_state: &LlmConversationState,
        attempt: i32,
    ) -> AppResult<ResponseEnvelope> {
        let policy_payload = self.parameter_payload(model)?;
        self.log_request(model, attempt, &policy_payload, conversation_state);
        let merged = LlmPromptContextRenderer::merge(user_prompt, prompt_context);
        let prompt = format!("{merged}\n\n{}", structured_output_format());
        self.request(
            api_key,
            model,
            system_prompt,
            &prompt,
            conversation_state,
            &policy_payload,
            attempt,
        )
        .map_err(|failure| match failure {
            RequestFailure::Status(body) => AppError::InvalidState(format!(
                "OpenAI request failed: {}",
                resolve_error_message(&body)
            )),
            RequestFailure::Other(error) => error,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn request(
        &self,
        api_key: &str,
        model: &str,
        system_prompt: &str,
        prompt: &str,
        conversation_state: &LlmConversationState,
        policy_payload: &Map<String, Value>,
        attempt: i32,
    ) -> Result<ResponseEnvelope, RequestFailure> {
        if self.responses_enabled {
            self.request_via_responses(
                api_key,
                model,
                system_prompt,
                prompt,
                conversation_state,
                policy_payload,
                attempt,
            )
        } else {
            self.request_via_legacy_chat(
                api_key,
                model,
                system_prompt,
                prompt,
                conversation_state,
                policy_payload,
                attempt,
            )
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn request_via_responses(
        &self,
        api_key: &str,
        model: &str,
        system_prompt: &str,
        prompt: &str,
        conversation_state: &LlmConversationState,
        policy_payload: &Map<String, Value>,
        attempt: i32,
    ) -> Result<ResponseEnvelope, RequestFailure> {
        let mut payload = Map::new();
        payload.insert("model".into(), json!(model));
        if !java_is_blank(system_prompt) {
            payload.insert("instructions".into(), json!(system_prompt));
        }
        payload.insert(
            "input".into(),
            json!([{
                "role": "user",
                "content": [{"type": "input_text", "text": prompt}]
            }]),
        );
        if let Some(temperature) = policy_payload.get("temperature") {
            payload.insert("temperature".into(), temperature.clone());
        }
        if conversation_state.has_conversation_id() {
            payload.insert(
                "conversation".into(),
                json!(conversation_state.conversation_id()),
            );
        }
        if !java_is_blank(conversation_state.response_id()) {
            payload.insert(
                "previous_response_id".into(),
                json!(conversation_state.response_id()),
            );
        }

        let response_body = self.post_json(api_key, RESPONSES_PATH, &payload)?;
        let root = parse_json(&response_body).map_err(RequestFailure::Other)?;
        let content = extract_responses_output_text(&root);
        let conversation_id = extract_conversation_id(&root, conversation_state);
        let response_id = safe(Some(&as_text(path(&root, "id"), "")));

        self.log_success(model, attempt, &content, &conversation_id);
        let next = LlmConversationState::new(
            &conversation_id,
            &response_id,
            conversation_state.turn_count_since_rebase() + 1,
        );
        Ok(ResponseEnvelope {
            content,
            conversation_state: next,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn request_via_legacy_chat(
        &self,
        api_key: &str,
        model: &str,
        system_prompt: &str,
        prompt: &str,
        conversation_state: &LlmConversationState,
        policy_payload: &Map<String, Value>,
        attempt: i32,
    ) -> Result<ResponseEnvelope, RequestFailure> {
        let mut payload = Map::new();
        payload.insert("model".into(), json!(model));
        let mut messages = Vec::new();
        if !java_is_blank(system_prompt) {
            messages.push(json!({"role": "system", "content": system_prompt}));
        }
        messages.push(json!({"role": "user", "content": prompt}));
        payload.insert("messages".into(), Value::Array(messages));
        if let Some(temperature) = policy_payload.get("temperature") {
            payload.insert("temperature".into(), temperature.clone());
        }

        let response_body = self.post_json(api_key, CHAT_COMPLETIONS_PATH, &payload)?;
        let root = parse_json(&response_body).map_err(RequestFailure::Other)?;
        let message = path(path_index(path(&root, "choices"), 0), "message");
        let content = safe(Some(&as_text(path(message, "content"), "")));
        let conversation_id = conversation_state.conversation_id().to_string();
        let response_id = safe(Some(&as_text(path(&root, "id"), "")));

        self.log_success(model, attempt, &content, &conversation_id);
        let next = LlmConversationState::new(
            &conversation_id,
            &response_id,
            conversation_state.turn_count_since_rebase() + 1,
        );
        Ok(ResponseEnvelope {
            content,
            conversation_state: next,
        })
    }

    /// 성공(2xx)이면 본문을, 그 밖의 상태면 `RequestFailure::Status(본문)`을 돌려준다.
    fn post_json(
        &self,
        api_key: &str,
        path: &str,
        payload: &Map<String, Value>,
    ) -> Result<String, RequestFailure> {
        let request = self
            .http
            .post(join_url(&self.base_url, path))
            .bearer_auth(api_key)
            .header("Content-Type", "application/json")
            .body(Value::Object(payload.clone()).to_string());
        let reply = http::send(request, Some(MAX_BODY_BYTES))
            .map_err(|err| RequestFailure::Other(err.into_app_error()))?;
        if reply.is_success() {
            Ok(reply.text())
        } else {
            Err(RequestFailure::Status(reply.text()))
        }
    }

    fn parameter_payload(&self, model: &str) -> AppResult<Map<String, Value>> {
        let mut payload = Map::new();
        payload.insert("model".into(), json!(model));
        self.policy_resolver.resolve(model)?.apply(&mut payload);
        Ok(payload)
    }

    fn log_request(
        &self,
        model: &str,
        attempt: i32,
        payload: &Map<String, Value>,
        conversation_state: &LlmConversationState,
    ) {
        if !cfg!(test) && !tracing::enabled!(tracing::Level::DEBUG) {
            return;
        }
        let optional = match payload.get("temperature") {
            Some(temperature) => format!("temperature={temperature}"),
            None => "none".to_string(),
        };
        self.emit(format!(
            "openai.llm.request model={model} attempt={attempt} conversation={} optionalParameters={optional}",
            default_message(conversation_state.conversation_id())
        ));
    }

    fn log_success(&self, model: &str, attempt: i32, response_body: &str, conversation_id: &str) {
        if !self.log_formatter.should_log_success_at_fine() {
            return;
        }
        if !cfg!(test) && !tracing::enabled!(tracing::Level::DEBUG) {
            return;
        }
        self.emit(format!(
            "openai.llm.response success model={model} attempt={attempt} conversation={} bodyPreview={}",
            default_message(conversation_id),
            self.log_formatter.response_preview(response_body)
        ));
    }

    fn emit(&self, line: String) {
        tracing::debug!("{line}");
        #[cfg(test)]
        self.captured_logs.lock().unwrap().push(line);
    }
}

impl LlmClient for OpenAiLlmClient {
    fn provider(&self) -> String {
        "openai".to_string()
    }

    fn bootstrap(
        &self,
        api_key: &str,
        model: &str,
        system_prompt: &str,
        conversation_state: &LlmConversationState,
    ) -> AppResult<LlmConversationState> {
        let resolved_model = resolve_chat_model(model);
        let policy_payload = self.parameter_payload(&resolved_model)?;
        self.log_request(&resolved_model, 0, &policy_payload, conversation_state);
        self.request(
            api_key,
            &resolved_model,
            system_prompt,
            BOOTSTRAP_INPUT_TEXT,
            conversation_state,
            &policy_payload,
            0,
        )
        .map(|envelope| envelope.conversation_state)
        .map_err(|failure| match failure {
            RequestFailure::Status(body) => AppError::InvalidState(format!(
                "OpenAI bootstrap failed: {}",
                resolve_error_message(&body)
            )),
            RequestFailure::Other(error) => error,
        })
    }

    fn generate(
        &self,
        api_key: &str,
        model: &str,
        system_prompt: &str,
        user_prompt: &str,
        conversation_state: &LlmConversationState,
        prompt_context: &LlmPromptContext,
    ) -> AppResult<LlmGenerateResult> {
        let resolved_model = resolve_chat_model(model);
        let mut last_failure: Option<(String, LlmConversationState)> = None;

        for attempt in 1..=MAX_STRUCTURED_ATTEMPTS {
            match self.structured_generate(
                api_key,
                &resolved_model,
                system_prompt,
                user_prompt,
                conversation_state,
                prompt_context,
                attempt,
            ) {
                Ok(result) => return Ok(result),
                Err(StructuredFailure::Fatal(error)) => return Err(error),
                Err(StructuredFailure::Output {
                    raw,
                    conversation_state,
                }) => {
                    tracing::warn!(
                        "openai.llm.structured_output failure model={resolved_model} attempt={attempt} reason=Structured output conversion failed"
                    );
                    last_failure = Some((raw, conversation_state));
                }
            }
        }

        match last_failure {
            Some((raw, state)) => Ok(self.fallback_from_structured_failure(&raw, state)),
            None => Err(AppError::InvalidState(
                "OpenAI structured output failed without fallback context".to_string(),
            )),
        }
    }
}

/// `StructuredFeedbackResponse`: 모델이 채워 주는 구조화 응답. 값이 없는 칸은 `None`이다.
#[derive(Debug, Default)]
struct StructuredFeedbackResponse {
    summary: Option<String>,
    corrections: Option<CorrectionsNode>,
    recommendations: Option<RecommendationsNode>,
    example_answer: Option<String>,
    rulebook_evidence: Option<Vec<Option<String>>>,
}

#[derive(Debug, Default)]
struct CorrectionsNode {
    grammar: Option<(Option<String>, Option<String>)>,
    expression: Option<(Option<String>, Option<String>)>,
    logic: Option<(Option<String>, Option<String>)>,
}

#[derive(Debug, Default)]
struct RecommendationsNode {
    filler: Option<(Option<String>, Option<String>)>,
    adjective: Option<(Option<String>, Option<String>)>,
    adverb: Option<(Option<String>, Option<String>)>,
}

/// `BeanOutputConverter.convert`: 코드 펜스를 벗기고 Jackson처럼 읽는다. 알 수 없는 필드는
/// 무시하고, 문자열 칸에는 숫자/불리언을 글자로 받으며, 객체나 배열은 받지 않는다.
fn convert_structured_output(text: &str) -> Result<StructuredFeedbackResponse, String> {
    let cleaned = strip_code_fence(text)?;
    let root = parse_first_json_value(&cleaned)?;
    let object = match &root {
        Value::Object(object) => object,
        other => return Err(format!("cannot deserialize {other} as an object")),
    };
    Ok(StructuredFeedbackResponse {
        summary: scalar_text(object.get("summary"))?,
        corrections: match object_member(object.get("corrections"))? {
            Some(node) => Some(CorrectionsNode {
                grammar: pair_member(node.get("grammar"), "issue", "fix")?,
                expression: pair_member(node.get("expression"), "issue", "fix")?,
                logic: pair_member(node.get("logic"), "issue", "fix")?,
            }),
            None => None,
        },
        recommendations: match object_member(object.get("recommendations"))? {
            Some(node) => Some(RecommendationsNode {
                filler: pair_member(node.get("filler"), "term", "usage")?,
                adjective: pair_member(node.get("adjective"), "term", "usage")?,
                adverb: pair_member(node.get("adverb"), "term", "usage")?,
            }),
            None => None,
        },
        example_answer: scalar_text(object.get("exampleAnswer"))?,
        rulebook_evidence: match object.get("rulebookEvidence") {
            None | Some(Value::Null) => None,
            Some(Value::Array(items)) => Some(
                items
                    .iter()
                    .map(|item| scalar_text(Some(item)))
                    .collect::<Result<_, _>>()?,
            ),
            Some(other) => return Err(format!("cannot deserialize {other} as a list")),
        },
    })
}

fn strip_code_fence(text: &str) -> Result<String, String> {
    let trimmed = java_trim(text);
    if !(trimmed.starts_with("```") && trimmed.ends_with("```")) {
        return Ok(trimmed.to_string());
    }
    let (first_line, rest) = match trimmed.split_once('\n') {
        Some((first, rest)) => (first, Some(rest)),
        None => (trimmed, None),
    };
    let body = if java_trim(first_line).eq_ignore_ascii_case("```json") {
        rest.unwrap_or("")
    } else {
        &trimmed[3..]
    };
    let body = body
        .len()
        .checked_sub(3)
        .and_then(|end| body.get(..end))
        .ok_or_else(|| "markdown fence is not closed".to_string())?;
    Ok(java_trim(body).to_string())
}

fn scalar_text(value: Option<&Value>) -> Result<Option<String>, String> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok(Some(text.clone())),
        Some(Value::Number(number)) => Ok(Some(number.to_string())),
        Some(Value::Bool(flag)) => Ok(Some(flag.to_string())),
        Some(other) => Err(format!("cannot deserialize {other} as a string")),
    }
}

fn object_member(value: Option<&Value>) -> Result<Option<&Map<String, Value>>, String> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Object(object)) => Ok(Some(object)),
        Some(other) => Err(format!("cannot deserialize {other} as an object")),
    }
}

type TextPair = (Option<String>, Option<String>);

fn pair_member(
    value: Option<&Value>,
    first: &str,
    second: &str,
) -> Result<Option<TextPair>, String> {
    object_member(value)?
        .map(|node| {
            Ok::<_, String>((
                scalar_text(node.get(first))?,
                scalar_text(node.get(second))?,
            ))
        })
        .transpose()
}

fn to_corrections(node: Option<&CorrectionsNode>) -> Corrections {
    let empty = CorrectionsNode::default();
    let node = node.unwrap_or(&empty);
    Corrections::new(
        correction_detail(node.grammar.as_ref()),
        correction_detail(node.expression.as_ref()),
        correction_detail(node.logic.as_ref()),
    )
}

fn to_recommendations(node: Option<&RecommendationsNode>) -> Recommendations {
    let empty = RecommendationsNode::default();
    let node = node.unwrap_or(&empty);
    Recommendations::new(
        recommendation_detail(node.filler.as_ref()),
        recommendation_detail(node.adjective.as_ref()),
        recommendation_detail(node.adverb.as_ref()),
    )
}

fn correction_detail(pair: Option<&TextPair>) -> CorrectionDetail {
    let (issue, fix) = pair.map_or((None, None), |(a, b)| (a.as_deref(), b.as_deref()));
    CorrectionDetail::new(&safe(issue), &safe(fix))
}

fn recommendation_detail(pair: Option<&TextPair>) -> RecommendationDetail {
    let (term, usage) = pair.map_or((None, None), |(a, b)| (a.as_deref(), b.as_deref()));
    RecommendationDetail::new(&safe(term), &safe(usage))
}

fn safe(value: Option<&str>) -> String {
    java_trim(value.unwrap_or("")).to_string()
}

fn safe_string_list(values: Option<&[Option<String>]>) -> Vec<String> {
    values
        .unwrap_or(&[])
        .iter()
        .flatten()
        .filter(|value| !java_is_blank(value))
        .map(|value| java_trim(value).to_string())
        .collect()
}

fn extract_responses_output_text(root: &Value) -> String {
    let output_text = path(root, "output_text");
    match output_text {
        Value::String(text) => return safe(Some(text)),
        Value::Array(items) => {
            let lines: Vec<String> = items
                .iter()
                .map(|item| safe(Some(&as_text(item, ""))))
                .filter(|line| !java_is_blank(line))
                .collect();
            if !lines.is_empty() {
                return lines.join("\n");
            }
        }
        _ => {}
    }

    let mut fragments = Vec::new();
    if let Value::Array(output) = path(root, "output") {
        for item in output {
            let Value::Array(content) = path(item, "content") else {
                continue;
            };
            for part in content {
                let text = safe(Some(&as_text(path(part, "text"), "")));
                if !java_is_blank(&text) {
                    fragments.push(text);
                }
            }
        }
    }
    fragments.join("\n")
}

fn extract_conversation_id(root: &Value, fallback: &LlmConversationState) -> String {
    let conversation = path(root, "conversation");
    match conversation {
        Value::String(text) => return safe(Some(text)),
        Value::Object(_) => {
            let from_object = safe(Some(&as_text(path(conversation, "id"), "")));
            if !java_is_blank(&from_object) {
                return from_object;
            }
        }
        _ => {}
    }
    let conversation_id = safe(Some(&as_text(path(root, "conversation_id"), "")));
    if !java_is_blank(&conversation_id) {
        return conversation_id;
    }
    safe(Some(fallback.conversation_id()))
}

/// 본문이 비어 있으면 `{}`로 읽는다(Java `body == null ? "{}"`와 `readTree("")`).
fn parse_json(body: &str) -> AppResult<Value> {
    if java_is_blank(body) {
        return Ok(Value::Null);
    }
    parse_first_json_value(body)
        .map_err(|_| AppError::InvalidState("OpenAI response parse failed".to_string()))
}

fn resolve_error_message(body: &str) -> String {
    if let Ok(root) = parse_json(body) {
        let message = safe(Some(&as_text(path(path(&root, "error"), "message"), "")));
        if !java_is_blank(&message) {
            return message;
        }
    }
    default_message(body)
}

fn resolve_chat_model(model: &str) -> String {
    let candidate = java_trim(model);
    if java_is_blank(candidate) || candidate.to_lowercase().contains("transcribe") {
        return DEFAULT_CHAT_MODEL.to_string();
    }
    candidate.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::OpenAiLoggingConfig;
    use httpmock::prelude::*;

    const API_KEY: &str = "api-key";

    fn config(server: &MockServer, responses_enabled: bool) -> OpenAiConfig {
        OpenAiConfig {
            base_url: format!("{}/", server.base_url()),
            responses_enabled,
            conversation_rebase_turns: 6,
            timeout_seconds: 300,
            logging: OpenAiLoggingConfig {
                response_preview_chars: 512,
                full_body: false,
                log_success_at_fine: true,
            },
        }
    }

    fn client(server: &MockServer) -> OpenAiLlmClient {
        OpenAiLlmClient::new(
            http::build_http_client(std::time::Duration::from_secs(300)).unwrap(),
            &config(server, true),
        )
    }

    fn responses_body(output_text: &str, response_id: &str, conversation_id: &str) -> String {
        json!({"id": response_id, "conversation": conversation_id, "output_text": output_text})
            .to_string()
    }

    fn minimal_structured_response() -> String {
        json!({
            "summary": "요약",
            "corrections": {
                "grammar": {"issue": "시제가 흔들림", "fix": "과거형으로 통일"},
                "expression": {"issue": "어휘가 단조로움", "fix": "구체 동사 사용"},
                "logic": {"issue": "근거가 부족함", "fix": "원인/결과 한 문장 추가"}
            },
            "recommendations": {
                "filler": {"term": "Well", "usage": "답변 시작 완충"},
                "adjective": {"term": "vivid", "usage": "경험 묘사 강화"},
                "adverb": {"term": "definitely", "usage": "확신 강조"}
            },
            "exampleAnswer": "example",
            "rulebookEvidence": ["[a.md] evidence"]
        })
        .to_string()
    }

    fn generate(client: &OpenAiLlmClient, model: &str) -> AppResult<LlmGenerateResult> {
        client.generate(
            API_KEY,
            model,
            "sys",
            "user",
            &LlmConversationState::empty(),
            &LlmPromptContext::empty(),
        )
    }

    fn expected_generate_body(model: &str, temperature: bool) -> Value {
        let mut body = json!({
            "model": model,
            "instructions": "sys",
            "input": [{
                "role": "user",
                "content": [{
                    "type": "input_text",
                    "text": format!("user\n\n{}", structured_output_format())
                }]
            }]
        });
        if temperature {
            body["temperature"] = json!(0.2);
        }
        body
    }

    #[test]
    fn provider_returns_open_ai() {
        let server = MockServer::start();
        assert_eq!("openai", client(&server).provider());
    }

    #[test]
    fn bootstrap_starts_conversation_with_instructions() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/responses")
                .header("authorization", "Bearer api-key")
                .header("content-type", "application/json")
                .json_body(json!({
                    "model": "gpt-4o-mini",
                    "instructions": "base coach prompt",
                    "input": [{
                        "role": "user",
                        "content": [{
                            "type": "input_text",
                            "text": "Acknowledge the coaching strategy and wait for the next user answer."
                        }]
                    }],
                    "temperature": 0.2
                }));
            then.status(200)
                .header("Content-Type", "application/json")
                .body(responses_body("ok", "resp-boot", "conv-boot"));
        });

        let state = client(&server)
            .bootstrap(
                API_KEY,
                "gpt-4o-mini",
                "base coach prompt",
                &LlmConversationState::empty(),
            )
            .unwrap();

        assert_eq!("conv-boot", state.conversation_id());
        assert_eq!("resp-boot", state.response_id());
        assert_eq!(1, state.turn_count_since_rebase());
        mock.assert();
    }

    #[test]
    fn generate_maps_structured_response() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/responses")
                .header("authorization", "Bearer api-key")
                .json_body(expected_generate_body("gpt-4o-mini", true));
            then.status(200)
                .header("Content-Type", "application/json")
                .body(responses_body(
                    &minimal_structured_response(),
                    "resp-1",
                    "conv-1",
                ));
        });

        let result = generate(&client(&server), "gpt-4o-mini").unwrap();

        assert_eq!("요약", result.feedback.summary());
        assert_eq!(
            "시제가 흔들림",
            result.feedback.corrections().grammar().issue()
        );
        assert_eq!("Well", result.feedback.recommendations().filler().term());
        assert_eq!("example", result.feedback.example_answer());
        assert_eq!(["[a.md] evidence"], result.feedback.rulebook_evidence());
        assert!(result.schema_fallback_reasons.is_empty());
        assert_eq!("conv-1", result.conversation_state.conversation_id());
        assert_eq!("resp-1", result.conversation_state.response_id());
        assert_eq!("요약", result.summary_for_next_turn);
        mock.assert();
    }

    #[test]
    fn generate_reuses_conversation_when_provided() {
        let server = MockServer::start();
        let mut expected = expected_generate_body("gpt-4o-mini", true);
        expected["conversation"] = json!("conv-1");
        expected["previous_response_id"] = json!("resp-1");
        let mock = server.mock(|when, then| {
            when.method(POST).path("/v1/responses").json_body(expected);
            then.status(200).body(responses_body(
                &minimal_structured_response(),
                "resp-2",
                "conv-1",
            ));
        });

        let result = client(&server)
            .generate(
                API_KEY,
                "gpt-4o-mini",
                "sys",
                "user",
                &LlmConversationState::new("conv-1", "resp-1", 1),
                &LlmPromptContext::empty(),
            )
            .unwrap();

        mock.assert();
        assert_eq!("resp-2", result.conversation_state.response_id());
        assert_eq!(2, result.conversation_state.turn_count_since_rebase());
    }

    #[test]
    fn generate_retries_and_falls_back_when_structured_conversion_fails() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path("/v1/responses");
            then.status(200)
                .body(responses_body("not-json", "resp-1", "conv-1"));
        });

        let result = generate(&client(&server), "gpt-4o-mini").unwrap();

        mock.assert_hits(2);
        assert!(result
            .schema_fallback_reasons
            .contains(&"structured_output_conversion_failed".to_string()));
        assert!(result
            .schema_fallback_reasons
            .contains(&"fallback_parse_failed".to_string()));
        assert_eq!("", result.feedback.summary());
        assert_eq!("conv-1", result.conversation_state.conversation_id());
    }

    #[test]
    fn generate_falls_back_to_lenient_parser_for_wrapped_json() {
        let server = MockServer::start();
        let wrapped = "Here you go: {\"summary\":\"wrapped\",\"corrections\":{\"grammar\":{\"issue\":[1],\"fix\":\"f\"}}} thanks";
        let mock = server.mock(|when, then| {
            when.method(POST).path("/v1/responses");
            then.status(200)
                .body(responses_body(wrapped, "resp-1", "conv-1"));
        });

        let result = generate(&client(&server), "gpt-4o-mini").unwrap();

        mock.assert_hits(2);
        assert_eq!(
            vec!["structured_output_conversion_failed".to_string()],
            result.schema_fallback_reasons
        );
        assert_eq!("wrapped", result.feedback.summary());
        assert_eq!("", result.feedback.corrections().grammar().issue());
        assert_eq!("f", result.feedback.corrections().grammar().fix());
        assert_eq!("wrapped", result.summary_for_next_turn);
    }

    #[test]
    fn generate_accepts_markdown_fenced_json() {
        let server = MockServer::start();
        let fenced = format!("```json\n{}\n```", minimal_structured_response());
        let mock = server.mock(|when, then| {
            when.method(POST).path("/v1/responses");
            then.status(200)
                .body(responses_body(&fenced, "resp-1", "conv-1"));
        });

        let result = generate(&client(&server), "gpt-4o-mini").unwrap();

        mock.assert_hits(1);
        assert!(result.schema_fallback_reasons.is_empty());
        assert_eq!("요약", result.feedback.summary());
    }

    #[test]
    fn generate_sends_temperature_for_legacy_models() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/responses")
                .json_body(expected_generate_body("gpt-4o-mini", true));
            then.status(200).body(responses_body(
                &minimal_structured_response(),
                "resp-1",
                "conv-1",
            ));
        });

        generate(&client(&server), "gpt-4o-mini").unwrap();

        mock.assert();
    }

    #[test]
    fn generate_does_not_send_temperature_for_gpt5_family() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/responses")
                .json_body(expected_generate_body("gpt-5-mini", false));
            then.status(200).body(responses_body(
                &minimal_structured_response(),
                "resp-1",
                "conv-1",
            ));
        });

        generate(&client(&server), "gpt-5-mini").unwrap();

        mock.assert();
    }

    #[test]
    fn generate_includes_recent_recommendation_history_in_merged_prompt() {
        let server = MockServer::start();
        let context = LlmPromptContext::new(
            "",
            vec![],
            vec![omypic_domain::session::RecommendationRecord::new(
                "well",
                "vivid",
                "definitely",
            )],
        );
        let merged = LlmPromptContextRenderer::merge("user", &context);
        let mut expected = expected_generate_body("gpt-4o-mini", true);
        expected["input"][0]["content"][0]["text"] =
            json!(format!("{merged}\n\n{}", structured_output_format()));
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/responses")
                .json_body(expected)
                .body_contains("Recent recommendation terms")
                .body_contains("Filler: well")
                .body_contains("# Current input");
            then.status(200).body(responses_body(
                &minimal_structured_response(),
                "resp-1",
                "conv-1",
            ));
        });

        client(&server)
            .generate(
                API_KEY,
                "gpt-4o-mini",
                "sys",
                "user",
                &LlmConversationState::empty(),
                &context,
            )
            .unwrap();

        mock.assert();
    }

    #[test]
    fn generate_uses_chat_completions_when_responses_are_disabled() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/chat/completions")
                .header("authorization", "Bearer api-key")
                .json_body(json!({
                    "model": "gpt-4o-mini",
                    "messages": [
                        {"role": "system", "content": "sys"},
                        {"role": "user", "content": format!("user\n\n{}", structured_output_format())}
                    ],
                    "temperature": 0.2
                }));
            then.status(200).body(
                json!({
                    "id": "chatcmpl-1",
                    "choices": [{"message": {"role": "assistant", "content": minimal_structured_response()}}]
                })
                .to_string(),
            );
        });
        let client = OpenAiLlmClient::new(
            http::build_http_client(std::time::Duration::from_secs(300)).unwrap(),
            &config(&server, false),
        );

        let result = client
            .generate(
                API_KEY,
                "gpt-4o-mini",
                "sys",
                "user",
                &LlmConversationState::new("conv-keep", "resp-old", 3),
                &LlmPromptContext::empty(),
            )
            .unwrap();

        mock.assert();
        assert_eq!("요약", result.feedback.summary());
        assert_eq!("conv-keep", result.conversation_state.conversation_id());
        assert_eq!("chatcmpl-1", result.conversation_state.response_id());
        assert_eq!(4, result.conversation_state.turn_count_since_rebase());
    }

    #[test]
    fn generate_omits_blank_instructions_and_defaults_transcribe_models() {
        let server = MockServer::start();
        let mut expected = expected_generate_body("gpt-4o-mini", true);
        expected.as_object_mut().unwrap().remove("instructions");
        let mock = server.mock(|when, then| {
            when.method(POST).path("/v1/responses").json_body(expected);
            then.status(200).body(responses_body(
                &minimal_structured_response(),
                "resp-1",
                "conv-1",
            ));
        });

        client(&server)
            .generate(
                API_KEY,
                "  gpt-4o-transcribe ",
                "  ",
                "user",
                &LlmConversationState::empty(),
                &LlmPromptContext::empty(),
            )
            .unwrap();

        mock.assert();
    }

    #[test]
    fn responses_output_falls_back_to_output_content_parts() {
        let root = json!({
            "output": [
                {"content": [{"text": " first "}, {"text": ""}]},
                {"content": "not-an-array"},
                {"content": [{"text": "second"}]}
            ]
        });
        assert_eq!("first\nsecond", extract_responses_output_text(&root));
        assert_eq!(
            "a\nb",
            extract_responses_output_text(&json!({"output_text": ["a", " ", "b"]}))
        );
    }

    #[test]
    fn conversation_id_prefers_text_then_object_then_field_then_fallback() {
        let fallback = LlmConversationState::new("fb", "", 0);
        assert_eq!(
            "c1",
            extract_conversation_id(&json!({"conversation": " c1 "}), &fallback)
        );
        assert_eq!(
            "c2",
            extract_conversation_id(&json!({"conversation": {"id": "c2"}}), &fallback)
        );
        assert_eq!(
            "c3",
            extract_conversation_id(&json!({"conversation_id": "c3"}), &fallback)
        );
        assert_eq!("fb", extract_conversation_id(&json!({}), &fallback));
    }

    #[test]
    fn http_error_status_maps_to_invalid_state_with_openai_message() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/v1/responses");
            then.status(400)
                .body(json!({"error": {"message": "Invalid conversation id: conv-1"}}).to_string());
        });

        let error = generate(&client(&server), "gpt-4o-mini").err().unwrap();

        assert_eq!(
            AppError::InvalidState("OpenAI request failed: Invalid conversation id: conv-1".into()),
            error
        );
    }

    #[test]
    fn http_error_without_json_message_uses_raw_body_or_unknown() {
        let server = MockServer::start();
        let mut raw = server.mock(|when, then| {
            when.method(POST).path("/v1/responses");
            then.status(502).body("bad gateway");
        });
        assert_eq!(
            AppError::InvalidState("OpenAI request failed: bad gateway".into()),
            generate(&client(&server), "gpt-4o-mini").err().unwrap()
        );
        raw.delete();
        server.mock(|when, then| {
            when.method(POST).path("/v1/responses");
            then.status(500);
        });
        assert_eq!(
            AppError::InvalidState("OpenAI request failed: unknown".into()),
            generate(&client(&server), "gpt-4o-mini").err().unwrap()
        );
    }

    #[test]
    fn bootstrap_http_error_uses_bootstrap_prefix() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/v1/responses");
            then.status(401)
                .body(json!({"error": {"message": "Incorrect API key"}}).to_string());
        });

        let error = client(&server)
            .bootstrap(
                API_KEY,
                "gpt-4o-mini",
                "sys",
                &LlmConversationState::empty(),
            )
            .err()
            .unwrap();

        assert_eq!(
            AppError::InvalidState("OpenAI bootstrap failed: Incorrect API key".into()),
            error
        );
    }

    #[test]
    fn unparseable_success_body_is_an_invalid_state() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/v1/responses");
            then.status(200).body("<html>");
        });

        let error = generate(&client(&server), "gpt-4o-mini").err().unwrap();

        assert_eq!(
            AppError::InvalidState("OpenAI response parse failed".into()),
            error
        );
    }

    #[test]
    fn connection_failure_is_an_internal_error_without_the_key() {
        let port = {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            listener.local_addr().unwrap().port()
        };
        let config = OpenAiConfig {
            base_url: format!("http://127.0.0.1:{port}"),
            responses_enabled: true,
            conversation_rebase_turns: 6,
            timeout_seconds: 300,
            logging: OpenAiLoggingConfig {
                response_preview_chars: 512,
                full_body: false,
                log_success_at_fine: true,
            },
        };
        let client = OpenAiLlmClient::new(
            http::build_http_client(std::time::Duration::from_secs(300)).unwrap(),
            &config,
        );

        let error = client
            .generate(
                "sk-test-secret",
                "gpt-4o-mini",
                "sys",
                "user",
                &LlmConversationState::empty(),
                &LlmPromptContext::empty(),
            )
            .err()
            .unwrap();

        let AppError::Internal(message) = error else {
            panic!("expected Internal, got {error:?}");
        };
        assert!(message.contains("error sending request"), "{message}");
        assert!(!message.contains("sk-test-secret"));
    }

    #[test]
    fn logged_lines_never_contain_the_api_key() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST)
                .path("/v1/responses")
                .header("authorization", "Bearer sk-test-secret");
            then.status(200).body(responses_body(
                &minimal_structured_response(),
                "resp-1",
                "conv-1",
            ));
        });
        let client = client(&server);

        client
            .bootstrap(
                "sk-test-secret",
                "gpt-4o-mini",
                "sys",
                &LlmConversationState::empty(),
            )
            .unwrap();
        client
            .generate(
                "sk-test-secret",
                "gpt-4o-mini",
                "sys",
                "user",
                &LlmConversationState::empty(),
                &LlmPromptContext::empty(),
            )
            .unwrap();

        let lines = client.captured_logs.lock().unwrap().clone();
        assert_eq!(4, lines.len());
        assert!(lines[0].starts_with("openai.llm.request model=gpt-4o-mini attempt=0 "));
        assert!(lines[1].starts_with("openai.llm.response success model=gpt-4o-mini attempt=0 "));
        assert!(lines[3].contains("bodyPreview="));
        for line in &lines {
            assert!(!line.contains("sk-test-secret"), "{line}");
            assert!(!line.to_lowercase().contains("authorization"), "{line}");
            assert!(!line.to_lowercase().contains("bearer"), "{line}");
        }
    }

    #[test]
    fn response_log_preview_is_truncated_to_configured_chars() {
        let server = MockServer::start();
        let long = "x".repeat(600);
        server.mock(|when, then| {
            when.method(POST).path("/v1/responses");
            then.status(200)
                .body(responses_body(&long, "resp-1", "conv-1"));
        });
        let client = client(&server);

        client
            .bootstrap(
                API_KEY,
                "gpt-4o-mini",
                "sys",
                &LlmConversationState::empty(),
            )
            .unwrap();

        let lines = client.captured_logs.lock().unwrap().clone();
        assert!(lines[1].ends_with(&format!("{}...(truncated)", "x".repeat(512))));
    }

    #[test]
    fn structured_output_format_embeds_the_schema_and_instructions() {
        let format = structured_output_format();
        assert!(format.starts_with("Your response should be in JSON format.\n"));
        assert!(format.contains("Do not include markdown code blocks in your response.\n"));
        assert!(format.ends_with("```\n"));
        let schema_start = format.find("```{").unwrap() + 3;
        let schema_end = format.rfind("}```").unwrap() + 1;
        let schema: Value = serde_json::from_str(&format[schema_start..schema_end]).unwrap();
        let mut properties: Vec<&str> = schema["properties"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        properties.sort_unstable();
        assert_eq!(
            vec![
                "corrections",
                "exampleAnswer",
                "recommendations",
                "rulebookEvidence",
                "summary"
            ],
            properties
        );
        assert_eq!(json!(false), schema["additionalProperties"]);
        assert_eq!(
            json!("#/$defs/CorrectionNode"),
            schema["properties"]["corrections"]["properties"]["logic"]["$ref"]
        );
    }

    #[test]
    fn structured_output_format_matches_the_spring_ai_template() {
        let expected = format!(
            "Your response should be in JSON format.\n\
             Do not include any explanations, only provide a RFC8259 compliant JSON response following this format without deviation.\n\
             Do not include markdown code blocks in your response.\n\
             Remove the ```json markdown from the output.\n\
             Here is the JSON Schema instance your output must adhere to:\n\
             ```{}```\n",
            STRUCTURED_SCHEMA
        );
        assert_eq!(expected, structured_output_format());
        assert!(STRUCTURED_SCHEMA.starts_with(
            "{\n  \"$schema\" : \"https://json-schema.org/draft/2020-12/schema\",\n  \"$defs\" : {\n    \"CorrectionNode\" : {"
        ));
        assert!(STRUCTURED_SCHEMA.ends_with("  \"additionalProperties\" : false\n}"));
    }

    #[test]
    fn convert_structured_output_follows_jackson_coercion_rules() {
        let numeric = convert_structured_output("{\"summary\": 5, \"unknown\": 1}").unwrap();
        assert_eq!(Some("5".to_string()), numeric.summary);
        assert!(convert_structured_output("{\"summary\": {\"a\": 1}}").is_err());
        assert!(convert_structured_output("[]").is_err());
        assert!(convert_structured_output("null").is_err());
        assert!(convert_structured_output("").is_err());
        assert!(convert_structured_output("```").is_err());
        assert!(convert_structured_output("{\"corrections\": \"x\"}").is_err());
        let tolerant = convert_structured_output("```\n{\"summary\":\"s\"} trailing\n```").unwrap();
        assert_eq!(Some("s".to_string()), tolerant.summary);
    }
}
