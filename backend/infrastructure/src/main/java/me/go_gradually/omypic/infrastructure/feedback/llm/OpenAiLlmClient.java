package me.go_gradually.omypic.infrastructure.feedback.llm;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import me.go_gradually.omypic.application.feedback.port.LlmClient;
import me.go_gradually.omypic.infrastructure.shared.config.AppProperties;
import org.springframework.beans.factory.annotation.Qualifier;
import org.springframework.stereotype.Component;
import org.springframework.web.reactive.function.client.WebClient;
import org.springframework.web.reactive.function.client.WebClientResponseException;

import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Set;
import java.util.logging.Level;
import java.util.logging.Logger;

@Component
public class OpenAiLlmClient implements LlmClient {
    private static final String DEFAULT_CHAT_MODEL = "gpt-4o-mini";
    private static final String CHAT_COMPLETIONS_ENDPOINT = "/v1/chat/completions";
    private static final Logger log = Logger.getLogger(OpenAiLlmClient.class.getName());
    private final WebClient webClient;
    private final ObjectMapper objectMapper = new ObjectMapper();
    private final OpenAiModelParameterPolicyResolver modelParameterPolicyResolver = new OpenAiModelParameterPolicyResolver();
    private final OpenAiLlmLogFormatter logFormatter;

    public OpenAiLlmClient(@Qualifier("openAiWebClient") WebClient webClient, AppProperties properties) {
        this.webClient = webClient;
        this.logFormatter = OpenAiLlmLogFormatter.from(properties);
    }

    @Override
    public String provider() {
        return "openai";
    }

    @Override
    public String generate(String apiKey, String model, String systemPrompt, String userPrompt) throws Exception {
        String response = requestOpenAi(apiKey, model, systemPrompt, userPrompt);
        return extractContent(response);
    }

    private String requestOpenAi(String apiKey, String model, String systemPrompt, String userPrompt) {
        Map<String, Object> payload = requestPayload(model, systemPrompt, userPrompt);
        try {
            return executeRequestWithLogging(apiKey, payload, 1);
        } catch (WebClientResponseException e) {
            if (e.getStatusCode().value() != 400) {
                throw e;
            }
            String unsupportedParam = unsupportedParameter(e.getResponseBodyAsString());
            if (!canRetryByRemoving(payload, unsupportedParam)) {
                throw e;
            }
            Map<String, Object> retriedPayload = new HashMap<>(payload);
            retriedPayload.remove(unsupportedParam);
            log.warning(() -> "openai request retry without unsupported parameter model="
                    + String.valueOf(payload.get("model"))
                    + " removed="
                    + unsupportedParam);
            return executeRequestWithLogging(apiKey, retriedPayload, 2);
        }
    }

    private String executeRequestWithLogging(String apiKey, Map<String, Object> payload, int attempt) {
        logRequest(payload, attempt);
        long startedAt = System.nanoTime();
        try {
            String response = requestOpenAiOnce(apiKey, payload);
            logSuccess(payload, attempt, startedAt, response);
            return response;
        } catch (WebClientResponseException e) {
            logFailure(payload, attempt, startedAt, e);
            throw e;
        } catch (RuntimeException e) {
            long latencyMs = elapsedMillis(startedAt);
            log.warning(() -> "openai.llm.response failure model="
                    + String.valueOf(payload.get("model"))
                    + " endpoint="
                    + CHAT_COMPLETIONS_ENDPOINT
                    + " attempt="
                    + attempt
                    + " latencyMs="
                    + latencyMs
                    + " reason="
                    + defaultMessage(e.getMessage()));
            throw e;
        }
    }

    private void logRequest(Map<String, Object> payload, int attempt) {
        if (!log.isLoggable(Level.FINE)) {
            return;
        }
        String model = String.valueOf(payload.get("model"));
        String optional = logFormatter.optionalParameterSummary(payload, modelParameterPolicyResolver.retryRemovableParameters());
        log.fine(() -> "openai.llm.request endpoint="
                + CHAT_COMPLETIONS_ENDPOINT
                + " model="
                + model
                + " attempt="
                + attempt
                + " optionalParameters="
                + optional);
    }

    private void logSuccess(Map<String, Object> payload, int attempt, long startedAt, String responseBody) {
        if (!logFormatter.shouldLogSuccessAtFine() || !log.isLoggable(Level.FINE)) {
            return;
        }
        long latencyMs = elapsedMillis(startedAt);
        log.fine(() -> "openai.llm.response success model="
                + String.valueOf(payload.get("model"))
                + " endpoint="
                + CHAT_COMPLETIONS_ENDPOINT
                + " attempt="
                + attempt
                + " status=200"
                + " latencyMs="
                + latencyMs
                + " bodyPreview="
                + logFormatter.responsePreview(responseBody));
    }

    private void logFailure(Map<String, Object> payload, int attempt, long startedAt, WebClientResponseException e) {
        long latencyMs = elapsedMillis(startedAt);
        String body = e.getResponseBodyAsString();
        OpenAiErrorInfo errorInfo = parseOpenAiError(body);
        log.warning(() -> "openai.llm.response failure model="
                + String.valueOf(payload.get("model"))
                + " endpoint="
                + CHAT_COMPLETIONS_ENDPOINT
                + " attempt="
                + attempt
                + " status="
                + e.getStatusCode().value()
                + " latencyMs="
                + latencyMs
                + " errorCode="
                + safeValue(errorInfo.code())
                + " errorParam="
                + safeValue(errorInfo.param())
                + " errorMessage="
                + safeValue(errorInfo.message())
                + " bodyPreview="
                + logFormatter.responsePreview(body));
    }

    private long elapsedMillis(long startedAt) {
        return (System.nanoTime() - startedAt) / 1_000_000L;
    }

    private String requestOpenAiOnce(String apiKey, Map<String, Object> payload) {
        return webClient.post()
                .uri(CHAT_COMPLETIONS_ENDPOINT)
                .header("Authorization", "Bearer " + apiKey)
                .bodyValue(payload)
                .retrieve()
                .bodyToMono(String.class)
                .block();
    }

    private Map<String, Object> requestPayload(String model, String systemPrompt, String userPrompt) {
        String resolvedModel = resolveChatModel(model);
        OpenAiModelParameterPolicy parameterPolicy = modelParameterPolicyResolver.resolve(resolvedModel);
        Map<String, Object> payload = new HashMap<>();
        payload.put("model", resolvedModel);
        parameterPolicy.apply(payload);
        payload.put("response_format", feedbackResponseFormat());
        payload.put("messages", List.of(
                Map.of("role", "system", "content", systemPrompt),
                Map.of("role", "user", "content", userPrompt)
        ));
        return payload;
    }

    private Map<String, Object> feedbackResponseFormat() {
        return Map.of("type", "json_schema", "json_schema", feedbackJsonSchema());
    }

    private Map<String, Object> feedbackJsonSchema() {
        return Map.of(
                "name", "feedback_response",
                "strict", true,
                "schema", feedbackSchema()
        );
    }

    private Map<String, Object> feedbackSchema() {
        return Map.of(
                "type", "object",
                "additionalProperties", false,
                "required", List.of("summary", "correctionPoints", "exampleAnswer", "rulebookEvidence"),
                "properties", schemaProperties()
        );
    }

    private Map<String, Object> schemaProperties() {
        Map<String, Object> properties = new HashMap<>();
        properties.put("summary", stringType());
        properties.put("correctionPoints", fixedStringArray(6));
        properties.put("exampleAnswer", stringType());
        properties.put("rulebookEvidence", stringArray());
        return properties;
    }

    private Map<String, Object> stringType() {
        return Map.of("type", "string");
    }

    private Map<String, Object> fixedStringArray(int count) {
        return Map.of(
                "type", "array",
                "minItems", count,
                "maxItems", count,
                "items", stringType()
        );
    }

    private Map<String, Object> stringArray() {
        return Map.of(
                "type", "array",
                "items", stringType()
        );
    }

    private String resolveChatModel(String model) {
        String candidate = model == null ? "" : model.trim();
        if (candidate.isBlank()) {
            return DEFAULT_CHAT_MODEL;
        }
        String lowered = candidate.toLowerCase(Locale.ROOT);
        if (lowered.contains("transcribe")) {
            return DEFAULT_CHAT_MODEL;
        }
        return candidate;
    }

    private boolean canRetryByRemoving(Map<String, Object> payload, String unsupportedParam) {
        if (unsupportedParam == null || unsupportedParam.isBlank()) {
            return false;
        }
        String normalized = normalizeParameterName(unsupportedParam);
        return modelParameterPolicyResolver.isRetryRemovableParameter(normalized) && payload.containsKey(normalized);
    }

    private String unsupportedParameter(String body) {
        if (body == null || body.isBlank()) {
            return null;
        }
        OpenAiErrorInfo info = parseOpenAiError(body);
        if ("unsupported_parameter".equalsIgnoreCase(info.code()) && !info.param().isBlank()) {
            return info.param();
        }
        return inferParameterFromMessage(info.message());
    }

    private OpenAiErrorInfo parseOpenAiError(String body) {
        if (body == null || body.isBlank()) {
            return new OpenAiErrorInfo("", "", "");
        }
        try {
            JsonNode root = objectMapper.readTree(body);
            JsonNode error = root.path("error");
            return new OpenAiErrorInfo(
                    error.path("code").asText(""),
                    normalizeParameterName(error.path("param").asText("")),
                    error.path("message").asText("")
            );
        } catch (Exception ignored) {
            return new OpenAiErrorInfo("", "", body);
        }
    }

    private String inferParameterFromMessage(String message) {
        if (message == null || message.isBlank()) {
            return null;
        }
        String lowered = message.toLowerCase(Locale.ROOT);
        Set<String> matched = new HashSet<>();
        for (String key : modelParameterPolicyResolver.retryRemovableParameters()) {
            if (lowered.contains(key.toLowerCase(Locale.ROOT))) {
                matched.add(key);
            }
        }
        if (matched.isEmpty()) {
            return null;
        }
        if (matched.contains("temperature")) {
            return "temperature";
        }
        return matched.stream().sorted().findFirst().orElse(null);
    }

    private String normalizeParameterName(String raw) {
        if (raw == null) {
            return "";
        }
        String normalized = raw.trim().toLowerCase(Locale.ROOT);
        if (normalized.startsWith("request.body.")) {
            normalized = normalized.substring("request.body.".length());
        }
        int dot = normalized.indexOf('.');
        if (dot >= 0) {
            normalized = normalized.substring(0, dot);
        }
        return normalized;
    }

    private String safeValue(String value) {
        return value == null ? "" : value;
    }

    private String defaultMessage(String message) {
        return message == null || message.isBlank() ? "unknown" : message;
    }

    private String extractContent(String response) throws Exception {
        JsonNode root = objectMapper.readTree(response);
        JsonNode content = root.path("choices").path(0).path("message").path("content");
        if (content.isMissingNode()) {
            throw new IllegalStateException("OpenAI response missing content");
        }
        return content.asText();
    }

    private record OpenAiErrorInfo(String code, String param, String message) {
    }
}
