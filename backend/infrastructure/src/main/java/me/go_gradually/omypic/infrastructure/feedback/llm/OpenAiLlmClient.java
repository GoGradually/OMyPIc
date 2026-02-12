package me.go_gradually.omypic.infrastructure.feedback.llm;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import me.go_gradually.omypic.application.feedback.port.LlmClient;
import org.springframework.beans.factory.annotation.Qualifier;
import org.springframework.stereotype.Component;
import org.springframework.web.reactive.function.client.WebClient;

import java.util.HashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;

@Component
public class OpenAiLlmClient implements LlmClient {
    private static final String DEFAULT_CHAT_MODEL = "gpt-4o-mini";
    private final WebClient webClient;
    private final ObjectMapper objectMapper = new ObjectMapper();

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
        return webClient.post()
                .uri("/v1/chat/completions")
                .header("Authorization", "Bearer " + apiKey)
                .bodyValue(payload)
                .retrieve()
                .bodyToMono(String.class)
                .block();
    }

    private Map<String, Object> requestPayload(String model, String systemPrompt, String userPrompt) {
        Map<String, Object> payload = new HashMap<>();
        payload.put("model", resolveChatModel(model));
        payload.put("temperature", 0.2);
        payload.put("response_format", Map.of("type", "json_object"));
        payload.put("messages", List.of(
                Map.of("role", "system", "content", systemPrompt),
                Map.of("role", "user", "content", userPrompt)
        ));
        return payload;
    }

    private String resolveChatModel(String model) {
        String candidate = model == null ? "" : model.trim();
        if (candidate.isBlank()) {
            return DEFAULT_CHAT_MODEL;
        }
        String lowered = candidate.toLowerCase(Locale.ROOT);
        if (lowered.startsWith("gpt-realtime") || lowered.contains("transcribe")) {
            return DEFAULT_CHAT_MODEL;
        }
        return candidate;
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
