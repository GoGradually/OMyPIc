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
public class GeminiLlmClient implements LlmClient {
    private final WebClient webClient;
    private final ObjectMapper objectMapper = new ObjectMapper();

    public GeminiLlmClient(@Qualifier("geminiWebClient") WebClient webClient) {
        this.webClient = webClient;
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
                .uri("/v1beta/models/" + model + ":generateContent?key=" + apiKey)
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
