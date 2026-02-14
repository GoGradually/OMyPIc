package me.go_gradually.omypic.infrastructure.feedback.llm;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import me.go_gradually.omypic.application.feedback.port.LlmClient;
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
import java.util.logging.Logger;

@Component
public class OpenAiLlmClient implements LlmClient {
    private static final String DEFAULT_CHAT_MODEL = "gpt-4o-mini";
    private static final Logger log = Logger.getLogger(OpenAiLlmClient.class.getName());
    private final WebClient webClient;
    private final ObjectMapper objectMapper = new ObjectMapper();
    private final OpenAiModelParameterPolicyResolver modelParameterPolicyResolver = new OpenAiModelParameterPolicyResolver();

    public OpenAiLlmClient(@Qualifier("openAiWebClient") WebClient webClient) {
        this.webClient = webClient;
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
            return requestOpenAiOnce(apiKey, payload);
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
            return requestOpenAiOnce(apiKey, retriedPayload);
        }
    }

    private String requestOpenAiOnce(String apiKey, Map<String, Object> payload) {
        return webClient.post()
                .uri("/v1/chat/completions")
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
        try {
            JsonNode root = objectMapper.readTree(body);
            JsonNode error = root.path("error");
            String code = error.path("code").asText("");
            String param = normalizeParameterName(error.path("param").asText(""));
            if ("unsupported_parameter".equalsIgnoreCase(code) && !param.isBlank()) {
                return param;
            }
            String message = error.path("message").asText("");
            return inferParameterFromMessage(message);
        } catch (Exception ignored) {
            return inferParameterFromMessage(body);
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

    private String extractContent(String response) throws Exception {
        JsonNode root = objectMapper.readTree(response);
        JsonNode content = root.path("choices").path(0).path("message").path("content");
        if (content.isMissingNode()) {
            throw new IllegalStateException("OpenAI response missing content");
        }
        return content.asText();
    }
}
