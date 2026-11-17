package me.go_gradually.omypic.infrastructure.feedback.llm;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import me.go_gradually.omypic.application.feedback.port.LlmGenerateResult;
import me.go_gradually.omypic.domain.session.LlmConversationState;
import me.go_gradually.omypic.domain.session.LlmPromptContext;
import me.go_gradually.omypic.infrastructure.shared.anthropic.AnthropicClientFactory;
import me.go_gradually.omypic.infrastructure.shared.config.AppProperties;
import okhttp3.mockwebserver.MockResponse;
import okhttp3.mockwebserver.MockWebServer;
import okhttp3.mockwebserver.RecordedRequest;
import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;

import java.util.List;
import java.util.Map;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

class AnthropicLlmClientTest {
    private static final String FEEDBACK_JSON = """
            {"summary":"Clear answer with one tense slip.",
             "corrections":{
               "grammar":{"issue":"I go there yesterday","fix":"I went there yesterday"},
               "expression":{"issue":"very good","fix":"really enjoyable"},
               "logic":{"issue":"no reason given","fix":"add why you liked it"}},
             "recommendations":{
               "filler":{"term":"Well","usage":"Open your answer with it."},
               "adjective":{"term":"peaceful","usage":"Describe the park."},
               "adverb":{"term":"definitely","usage":"Stress your opinion."}},
             "exampleAnswer":"Well, I went to a peaceful park yesterday.",
             "rulebookEvidence":["[travel.md] use past tense"]}
            """;

    private final ObjectMapper objectMapper = new ObjectMapper();
    private MockWebServer server;
    private AnthropicClientFactory clientFactory;

    @BeforeEach
    void setUp() throws Exception {
        server = new MockWebServer();
        server.start();
    }

    @AfterEach
    void tearDown() throws Exception {
        if (clientFactory != null) {
            clientFactory.close();
        }
        server.shutdown();
    }

    @Test
    void provider_isAnthropicAndStateless() {
        AnthropicLlmClient client = client("low");

        assertEquals("anthropic", client.provider());
        assertTrue(client.isStateless());
    }

    @Test
    void bootstrap_makesNoRequest() {
        AnthropicLlmClient client = client("low");

        LlmConversationState state = client.bootstrap("sk-ant-key", "claude-opus-5-5", "system", LlmConversationState.empty());

        assertFalse(state.hasConversationId());
        assertEquals(0, server.getRequestCount());
    }

    @Test
    void generate_sendsStructuredOutputRequestAndParsesFeedback() throws Exception {
        enqueueMessage("claude-opus-5-5", "end_turn", FEEDBACK_JSON);
        AnthropicLlmClient client = client("medium");

        LlmGenerateResult result = client.generate(
                "sk-ant-key",
                "claude-opus-5-5",
                "coach system prompt",
                "Question: park\nAnswer: I go there yesterday",
                LlmConversationState.empty(),
                LlmPromptContext.empty()
        );

        assertEquals("Clear answer with one tense slip.", result.feedback().getSummary());
        assertEquals("I went there yesterday", result.feedback().getCorrections().grammar().fix());
        assertEquals("peaceful", result.feedback().getRecommendations().adjective().term());
        assertTrue(result.schemaFallbackReasons().isEmpty());
        assertFalse(result.conversationState().hasConversationId());

        RecordedRequest request = server.takeRequest();
        assertEquals("POST", request.getMethod());
        assertTrue(request.getPath().startsWith("/v1/messages"));
        assertEquals("Bearer sk-ant-key", request.getHeader("Authorization"));
        JsonNode payload = objectMapper.readTree(request.getBody().readUtf8());
        assertEquals("claude-opus-5-5", payload.path("model").asText());
        assertEquals(16000, payload.path("max_tokens").asInt());
        assertEquals("coach system prompt", payload.path("system").path(0).path("text").asText());
        assertEquals("ephemeral", payload.path("system").path(0).path("cache_control").path("type").asText());
        assertEquals("medium", payload.path("output_config").path("effort").asText());
        JsonNode format = payload.path("output_config").path("format");
        assertEquals("json_schema", format.path("type").asText());
        assertEquals("object", format.path("schema").path("type").asText());
        assertFalse(format.path("schema").path("additionalProperties").asBoolean(true));
        assertEquals(5, format.path("schema").path("required").size());
        assertTrue(format.path("schema").path("properties").path("corrections").path("properties").has("grammar"));
        assertTrue(payload.path("thinking").isMissingNode());
        assertTrue(payload.path("temperature").isMissingNode());
    }

    @Test
    void generate_enablesServerSideRefusalFallback_forModelsThatSupportIt() throws Exception {
        enqueueMessage("claude-opus-5-5", "end_turn", FEEDBACK_JSON);
        AnthropicLlmClient client = client("low");

        client.generate("sk-ant-key", "claude-opus-5-5", "system", "prompt", LlmConversationState.empty(), null);

        RecordedRequest request = server.takeRequest();
        assertTrue(String.valueOf(request.getHeader("anthropic-beta")).contains("server-side-fallback-2026-07-01"));
        JsonNode payload = objectMapper.readTree(request.getBody().readUtf8());
        assertEquals("default", payload.path("fallbacks").asText());
    }

    @Test
    void generate_omitsFallback_forHaiku() throws Exception {
        enqueueMessage("claude-haiku-5-5", "end_turn", FEEDBACK_JSON);
        AnthropicLlmClient client = client("low");

        client.generate("sk-ant-key", "claude-haiku-5-5", "system", "prompt", LlmConversationState.empty(), null);

        RecordedRequest request = server.takeRequest();
        JsonNode payload = objectMapper.readTree(request.getBody().readUtf8());
        assertTrue(payload.path("fallbacks").isMissingNode());
        assertFalse(String.valueOf(request.getHeader("anthropic-beta")).contains("server-side-fallback"));
    }

    @Test
    void generate_mergesSessionMemoryIntoUserPrompt() throws Exception {
        enqueueMessage("claude-sonnet-5-5", "end_turn", FEEDBACK_JSON);
        AnthropicLlmClient client = client("low");
        LlmPromptContext context = new LlmPromptContext(
                "learner mixes tenses",
                List.of(new LlmPromptContext.TurnRecord("Q1", "A1", "summary-1")),
                List.of(new LlmPromptContext.RecommendationRecord("Well", "peaceful", "definitely"))
        );

        client.generate("sk-ant-key", "claude-sonnet-5-5", "system", "current turn", LlmConversationState.empty(), context);

        JsonNode payload = objectMapper.readTree(server.takeRequest().getBody().readUtf8());
        String userText = payload.path("messages").path(0).path("content").toString();
        assertTrue(userText.contains("learner mixes tenses"));
        assertTrue(userText.contains("Recent turns"));
        assertTrue(userText.contains("current turn"));
    }

    @Test
    void generate_throws_whenClaudeDeclines() {
        enqueueRefusal();
        AnthropicLlmClient client = client("low");

        IllegalStateException error = assertThrows(IllegalStateException.class, () -> client.generate(
                "sk-ant-key", "claude-haiku-5-5", "system", "prompt", LlmConversationState.empty(), null
        ));

        assertTrue(error.getMessage().contains("declined"));
    }

    @Test
    void generate_reportsSchemaFallback_whenReplyIsCutOff() {
        enqueueMessage("claude-haiku-5-5", "max_tokens", "{\"summary\":\"cut");
        AnthropicLlmClient client = client("low");

        LlmGenerateResult result = client.generate(
                "sk-ant-key", "claude-haiku-5-5", "system", "prompt", LlmConversationState.empty(), null
        );

        assertEquals(List.of("structured_output_conversion_failed"), result.schemaFallbackReasons());
        assertEquals("", result.feedback().getSummary());
    }

    @Test
    void generate_explainsRejectedApiKey() {
        server.enqueue(new MockResponse()
                .setResponseCode(401)
                .setHeader("Content-Type", "application/json")
                .setBody("{\"type\":\"error\",\"error\":{\"type\":\"authentication_error\",\"message\":\"invalid x-api-key\"}}"));
        AnthropicLlmClient client = client("low");

        IllegalStateException error = assertThrows(IllegalStateException.class, () -> client.generate(
                "sk-ant-bad", "claude-haiku-5-5", "system", "prompt", LlmConversationState.empty(), null
        ));

        assertEquals("Claude request failed: the Anthropic API key was rejected", error.getMessage());
    }

    private AnthropicLlmClient client(String effort) {
        AppProperties properties = new AppProperties();
        properties.getIntegrations().getAnthropic().setBaseUrl(server.url("/").toString());
        properties.getIntegrations().getAnthropic().setEffort(effort);
        properties.getIntegrations().getAnthropic().setMaxRetries(0);
        clientFactory = new AnthropicClientFactory(properties);
        return new AnthropicLlmClient(properties, clientFactory);
    }

    private void enqueueMessage(String model, String stopReason, String text) {
        server.enqueue(jsonResponse(Map.of(
                "id", "msg_test",
                "type", "message",
                "role", "assistant",
                "model", model,
                "content", List.of(Map.of("type", "text", "text", text)),
                "stop_reason", stopReason,
                "usage", Map.of("input_tokens", 120, "output_tokens", 80)
        )));
    }

    private void enqueueRefusal() {
        server.enqueue(jsonResponse(Map.of(
                "id", "msg_refused",
                "type", "message",
                "role", "assistant",
                "model", "claude-haiku-5-5",
                "content", List.of(),
                "stop_reason", "refusal",
                "stop_details", Map.of("type", "refusal", "category", "cyber", "explanation", "declined"),
                "usage", Map.of("input_tokens", 10, "output_tokens", 0)
        )));
    }

    private MockResponse jsonResponse(Map<String, Object> body) {
        try {
            return new MockResponse()
                    .setResponseCode(200)
                    .setHeader("Content-Type", "application/json")
                    .setBody(objectMapper.writeValueAsString(body));
        } catch (Exception e) {
            throw new IllegalStateException(e);
        }
    }
}
