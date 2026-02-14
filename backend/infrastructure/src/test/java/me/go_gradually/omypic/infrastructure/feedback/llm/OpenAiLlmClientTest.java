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
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

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
        OpenAiLlmClient client = new OpenAiLlmClient(WebClient.builder()
                .baseUrl(server.url("/").toString())
                .build());

        assertEquals("openai", client.provider());
    }

    @Test
    void generate_sendsRequestAndParsesContent() throws Exception {
        server.enqueue(new MockResponse()
                .setHeader("Content-Type", "application/json")
                .setBody("{\"choices\":[{\"message\":{\"content\":\"{\\\"summary\\\":\\\"ok\\\"}\"}}]}"));

        OpenAiLlmClient client = new OpenAiLlmClient(WebClient.builder()
                .baseUrl(server.url("/").toString())
                .build());
        String result = client.generate("api-key", "gpt-4o-mini", "sys", "user");

        assertEquals("{\"summary\":\"ok\"}", result);

        RecordedRequest request = server.takeRequest();
        assertEquals("POST", request.getMethod());
        assertEquals("/v1/chat/completions", request.getPath());
        assertEquals("Bearer api-key", request.getHeader("Authorization"));

        JsonNode payload = objectMapper.readTree(request.getBody().readUtf8());
        assertEquals("gpt-4o-mini", payload.path("model").asText());
        assertEquals(0.2, payload.path("temperature").asDouble());
        assertEquals("sys", payload.path("messages").path(0).path("content").asText());
        assertEquals("user", payload.path("messages").path(1).path("content").asText());
        assertEquals("json_schema", payload.path("response_format").path("type").asText());
        JsonNode schema = payload.path("response_format").path("json_schema").path("schema");
        assertEquals("object", schema.path("type").asText());
        assertEquals(false, schema.path("additionalProperties").asBoolean());
        JsonNode correctionPoints = schema.path("properties").path("correctionPoints");
        assertEquals("array", correctionPoints.path("type").asText());
        assertEquals(6, correctionPoints.path("minItems").asInt());
        assertEquals(6, correctionPoints.path("maxItems").asInt());
    }

    @Test
    void generate_throwsWhenContentMissing() {
        server.enqueue(new MockResponse()
                .setHeader("Content-Type", "application/json")
                .setBody("{\"choices\":[]}"));

        OpenAiLlmClient client = new OpenAiLlmClient(WebClient.builder()
                .baseUrl(server.url("/").toString())
                .build());

        assertThrows(IllegalStateException.class,
                () -> client.generate("api-key", "gpt-4o-mini", "sys", "user"));
    }

    @Test
    void generate_remapsTranscribeModelToChatModel() throws Exception {
        server.enqueue(new MockResponse()
                .setHeader("Content-Type", "application/json")
                .setBody("{\"choices\":[{\"message\":{\"content\":\"{\\\"summary\\\":\\\"ok\\\"}\"}}]}"));

        OpenAiLlmClient client = new OpenAiLlmClient(WebClient.builder()
                .baseUrl(server.url("/").toString())
                .build());

        client.generate("api-key", "gpt-4o-mini-transcribe", "sys", "user");

        JsonNode payload = objectMapper.readTree(server.takeRequest().getBody().readUtf8());
        assertEquals("gpt-4o-mini", payload.path("model").asText());
    }

    @Test
    void generate_doesNotSendTemperatureForGpt5Family() throws Exception {
        server.enqueue(new MockResponse()
                .setHeader("Content-Type", "application/json")
                .setBody("{\"choices\":[{\"message\":{\"content\":\"{\\\"summary\\\":\\\"ok\\\"}\"}}]}"));

        OpenAiLlmClient client = new OpenAiLlmClient(WebClient.builder()
                .baseUrl(server.url("/").toString())
                .build());

        client.generate("api-key", "gpt-5-mini", "sys", "user");

        JsonNode payload = objectMapper.readTree(server.takeRequest().getBody().readUtf8());
        assertEquals("gpt-5-mini", payload.path("model").asText());
        assertFalse(payload.has("temperature"));
    }

    @Test
    void generate_retriesOnceWithoutUnsupportedParameter() throws Exception {
        server.enqueue(new MockResponse()
                .setResponseCode(400)
                .setHeader("Content-Type", "application/json")
                .setBody("""
                        {
                          "error": {
                            "message": "Unsupported parameter: 'temperature' is not supported with this model.",
                            "type": "invalid_request_error",
                            "param": "temperature",
                            "code": "unsupported_parameter"
                          }
                        }
                        """));
        server.enqueue(new MockResponse()
                .setHeader("Content-Type", "application/json")
                .setBody("{\"choices\":[{\"message\":{\"content\":\"{\\\"summary\\\":\\\"ok\\\"}\"}}]}"));

        OpenAiLlmClient client = new OpenAiLlmClient(WebClient.builder()
                .baseUrl(server.url("/").toString())
                .build());

        String result = client.generate("api-key", "gpt-4o-mini", "sys", "user");
        assertEquals("{\"summary\":\"ok\"}", result);

        RecordedRequest first = server.takeRequest();
        RecordedRequest second = server.takeRequest();

        JsonNode firstPayload = objectMapper.readTree(first.getBody().readUtf8());
        JsonNode secondPayload = objectMapper.readTree(second.getBody().readUtf8());
        assertTrue(firstPayload.has("temperature"));
        assertFalse(secondPayload.has("temperature"));
        assertEquals("gpt-4o-mini", secondPayload.path("model").asText());
    }
}
