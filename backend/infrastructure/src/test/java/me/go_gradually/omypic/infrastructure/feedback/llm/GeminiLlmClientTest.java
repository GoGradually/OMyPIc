package me.go_gradually.omypic.infrastructure.feedback.llm;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import okhttp3.mockwebserver.MockResponse;
import okhttp3.mockwebserver.MockWebServer;
import okhttp3.mockwebserver.RecordedRequest;
import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.springframework.web.reactive.function.client.WebClient;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;

class GeminiLlmClientTest {

    private final ObjectMapper objectMapper = new ObjectMapper();

    private MockWebServer server;

    @BeforeEach
    void setUp() throws Exception {
        server = new MockWebServer();
        server.start();
    }

    @AfterEach
    void tearDown() throws Exception {
        server.shutdown();
    }

    @Test
    void provider_returnsGemini() {
        GeminiLlmClient client = new GeminiLlmClient(WebClient.builder().build(), template());

        assertEquals("gemini", client.provider());
    }

    @Test
    void generate_sendsRequestAndParsesContent() throws Exception {
        server.enqueue(new MockResponse()
                .setHeader("Content-Type", "application/json")
                .setBody("{\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"result\"}]}}]}"));

        GeminiLlmClient client = new GeminiLlmClient(WebClient.builder().build(), template());
        String result = client.generate("api-key", "gemini-1.5", "system prompt", "user prompt");

        assertEquals("result", result);

        RecordedRequest request = server.takeRequest();
        assertEquals("POST", request.getMethod());
        assertEquals("/v1beta/models/gemini-1.5:generateContent?key=api-key", request.getPath());

        JsonNode payload = objectMapper.readTree(request.getBody().readUtf8());
        String text = payload.path("contents").path(0).path("parts").path(0).path("text").asText();
        assertEquals("system prompt\n\nuser prompt", text);
    }

    @Test
    void generate_throwsWhenContentMissing() {
        server.enqueue(new MockResponse()
                .setHeader("Content-Type", "application/json")
                .setBody("{\"candidates\":[]}"));

        GeminiLlmClient client = new GeminiLlmClient(WebClient.builder().build(), template());

        assertThrows(IllegalStateException.class,
                () -> client.generate("api-key", "gemini-1.5", "system", "user"));
    }

    private String template() {
        return server.url("/v1beta/models/") + "%s:generateContent?key=%s";
    }
}
