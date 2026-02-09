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
public class GeminiLlmClient implements LlmClient {
    private static final String DEFAULT_ENDPOINT_TEMPLATE =
            "https://generativelanguage.googleapis.com/v1beta/models/%s:generateContent?key=%s";

    private final WebClient webClient;
    private final ObjectMapper objectMapper = new ObjectMapper();
    private final String endpointTemplate;

    public GeminiLlmClient(WebClient webClient) {
        this(webClient, DEFAULT_ENDPOINT_TEMPLATE);
    }

    GeminiLlmClient(WebClient webClient, String endpointTemplate) {
        this.webClient = webClient;
        this.endpointTemplate = endpointTemplate;
    }

    @Override
    public String provider() {
        return "gemini";
    }

    @Override
    public String generate(String apiKey, String model, String systemPrompt, String userPrompt) throws Exception {
        Map<String, Object> payload = new HashMap<>();
        String combined = systemPrompt + "\n\n" + userPrompt;
        payload.put("contents", List.of(Map.of(
                "role", "user",
                "parts", List.of(Map.of("text", combined))
        )));
        payload.put("generationConfig", Map.of("temperature", 0.2));

        String response = webClient.post()
                .uri(String.format(endpointTemplate, model, apiKey))
                .bodyValue(payload)
                .retrieve()
                .bodyToMono(String.class)
                .block();

        JsonNode root = objectMapper.readTree(response);
        JsonNode content = root.path("candidates").path(0).path("content").path("parts").path(0).path("text");
        if (content.isMissingNode()) {
            throw new IllegalStateException("Gemini response missing content");
        }
        return content.asText();
    }
}
