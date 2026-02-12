package me.go_gradually.omypic.infrastructure.feedback.llm;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import me.go_gradually.omypic.application.feedback.port.LlmClient;
import org.springframework.beans.factory.annotation.Qualifier;
import org.springframework.stereotype.Component;
import org.springframework.web.reactive.function.client.WebClient;

import java.util.HashMap;
import java.util.List;
import java.util.Map;

@Component
public class AnthropicLlmClient implements LlmClient {
    private final WebClient webClient;
    private final ObjectMapper objectMapper = new ObjectMapper();

    public AnthropicLlmClient(@Qualifier("anthropicWebClient") WebClient webClient) {
        this.webClient = webClient;
    }

    @Override
    public String provider() {
        return "anthropic";
    }

    @Override
    public String generate(String apiKey, String model, String systemPrompt, String userPrompt) throws Exception {
        String response = requestAnthropic(apiKey, model, systemPrompt, userPrompt);
        return extractContent(response);
    }

    private String requestAnthropic(String apiKey, String model, String systemPrompt, String userPrompt) {
        Map<String, Object> payload = requestPayload(model, systemPrompt, userPrompt);
        return webClient.post()
                .uri("/v1/messages")
                .header("x-api-key", apiKey)
                .header("anthropic-version", "2023-06-01")
                .bodyValue(payload)
                .retrieve()
                .bodyToMono(String.class)
                .block();
    }

    private Map<String, Object> requestPayload(String model, String systemPrompt, String userPrompt) {
        Map<String, Object> payload = new HashMap<>();
        payload.put("model", model);
        payload.put("max_tokens", 1024);
        payload.put("system", systemPrompt);
        payload.put("messages", List.of(Map.of("role", "user", "content", userPrompt)));
        return payload;
    }

    private String extractContent(String response) throws Exception {
        JsonNode root = objectMapper.readTree(response);
        JsonNode content = root.path("content").path(0).path("text");
        if (content.isMissingNode()) {
            throw new IllegalStateException("Anthropic response missing content");
        }
        return content.asText();
    }
}
