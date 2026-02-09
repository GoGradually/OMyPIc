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

class OpenAiLlmClientTest {

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
    void provider_returnsOpenAi() {
        OpenAiLlmClient client = new OpenAiLlmClient(WebClient.builder().build(), server.url("/chat/completions").toString());

        assertEquals("openai", client.provider());
    }

    @Test
    void generate_sendsRequestAndParsesContent() throws Exception {
        server.enqueue(new MockResponse()
                .setHeader("Content-Type", "application/json")
                .setBody("{\"choices\":[{\"message\":{\"content\":\"{\\\"summary\\\":\\\"ok\\\"}\"}}]}"));

        OpenAiLlmClient client = new OpenAiLlmClient(WebClient.builder().build(), server.url("/chat/completions").toString());
        String result = client.generate("api-key", "gpt-4o-mini", "sys", "user");

        assertEquals("{\"summary\":\"ok\"}", result);

        RecordedRequest request = server.takeRequest();
        assertEquals("POST", request.getMethod());
        assertEquals("/chat/completions", request.getPath());
        assertEquals("Bearer api-key", request.getHeader("Authorization"));

        JsonNode payload = objectMapper.readTree(request.getBody().readUtf8());
        assertEquals("gpt-4o-mini", payload.path("model").asText());
        assertEquals("sys", payload.path("messages").path(0).path("content").asText());
        assertEquals("user", payload.path("messages").path(1).path("content").asText());
    }

    @Test
    void generate_throwsWhenContentMissing() {
        server.enqueue(new MockResponse()
                .setHeader("Content-Type", "application/json")
                .setBody("{\"choices\":[]}"));

        OpenAiLlmClient client = new OpenAiLlmClient(WebClient.builder().build(), server.url("/chat/completions").toString());

        assertThrows(IllegalStateException.class,
                () -> client.generate("api-key", "gpt-4o-mini", "sys", "user"));
    }
}
