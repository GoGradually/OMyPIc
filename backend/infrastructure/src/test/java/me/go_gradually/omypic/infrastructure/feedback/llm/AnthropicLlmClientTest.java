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

class AnthropicLlmClientTest {

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
    void provider_returnsAnthropic() {
        AnthropicLlmClient client = new AnthropicLlmClient(WebClient.builder()
                .baseUrl(server.url("/").toString())
                .build());

        assertEquals("anthropic", client.provider());
    }

    @Test
    void generate_sendsRequestAndParsesContent() throws Exception {
        server.enqueue(new MockResponse()
                .setHeader("Content-Type", "application/json")
                .setBody("{\"content\":[{\"text\":\"hello\"}]}"));

        AnthropicLlmClient client = new AnthropicLlmClient(WebClient.builder()
                .baseUrl(server.url("/").toString())
                .build());
        String result = client.generate("api-key", "claude", "sys", "user");

        assertEquals("hello", result);

        RecordedRequest request = server.takeRequest();
        assertEquals("POST", request.getMethod());
        assertEquals("/v1/messages", request.getPath());
        assertEquals("api-key", request.getHeader("x-api-key"));
        assertEquals("2023-06-01", request.getHeader("anthropic-version"));

        JsonNode payload = objectMapper.readTree(request.getBody().readUtf8());
        assertEquals("claude", payload.path("model").asText());
        assertEquals("sys", payload.path("system").asText());
        assertEquals("user", payload.path("messages").path(0).path("content").asText());
    }

    @Test
    void generate_throwsWhenContentMissing() {
        server.enqueue(new MockResponse()
                .setHeader("Content-Type", "application/json")
                .setBody("{\"content\":[]}"));

        AnthropicLlmClient client = new AnthropicLlmClient(WebClient.builder()
                .baseUrl(server.url("/").toString())
                .build());

        assertThrows(IllegalStateException.class,
                () -> client.generate("api-key", "claude", "sys", "user"));
    }
}
