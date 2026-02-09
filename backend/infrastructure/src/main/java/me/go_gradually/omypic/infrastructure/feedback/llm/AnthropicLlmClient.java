package me.go_gradually.omypic.infrastructure.feedback.llm;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import me.go_gradually.omypic.application.feedback.port.LlmClient;
import org.springframework.stereotype.Component;
import org.springframework.web.reactive.function.client.WebClient;

import java.util.HashMap;
import java.util.List;
import java.util.Map;

@Component
public class AnthropicLlmClient implements LlmClient {
    private static final String DEFAULT_ENDPOINT = "https://api.anthropic.com/v1/messages";

    private final WebClient webClient;
    private final ObjectMapper objectMapper = new ObjectMapper();
    private final String endpoint;

    public AnthropicLlmClient(WebClient webClient) {
        this(webClient, DEFAULT_ENDPOINT);
    }

    AnthropicLlmClient(WebClient webClient, String endpoint) {
        this.webClient = webClient;
        this.endpoint = endpoint;
    }

    @Override
    public String provider() {
        return "anthropic";
    }

    @Override
    public String generate(String apiKey, String model, String systemPrompt, String userPrompt) throws Exception {
        Map<String, Object> payload = new HashMap<>();
        payload.put("model", model);
        payload.put("max_tokens", 1024);
        payload.put("system", systemPrompt);
        payload.put("messages", List.of(Map.of("role", "user", "content", userPrompt)));

        String response = webClient.post()
                .uri(endpoint)
                .header("x-api-key", apiKey)
                .header("anthropic-version", "2023-06-01")
                .bodyValue(payload)
                .retrieve()
                .bodyToMono(String.class)
                .block();

        JsonNode root = objectMapper.readTree(response);
        JsonNode content = root.path("content").path(0).path("text");
        if (content.isMissingNode()) {
            throw new IllegalStateException("Anthropic response missing content");
        }
        return content.asText();
    }
}
