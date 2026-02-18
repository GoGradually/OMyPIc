package me.go_gradually.omypic.infrastructure.feedback.llm;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import me.go_gradually.omypic.domain.session.LlmConversationState;
import me.go_gradually.omypic.application.feedback.port.LlmGenerateResult;
import me.go_gradually.omypic.domain.session.LlmPromptContext;
import me.go_gradually.omypic.infrastructure.shared.config.AppProperties;
import okhttp3.mockwebserver.MockResponse;
import okhttp3.mockwebserver.MockWebServer;
import okhttp3.mockwebserver.RecordedRequest;
import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;

import java.util.List;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
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
        OpenAiLlmClient client = client();
        assertEquals("openai", client.provider());
    }

    @Test
    void bootstrap_startsConversationWithInstructions() throws Exception {
        enqueueResponsesResponse("ok", "resp-boot", "conv-boot");

        OpenAiLlmClient client = client();
        LlmConversationState state = client.bootstrap(
                "api-key",
                "gpt-4o-mini",
                "base coach prompt",
                LlmConversationState.empty()
        );

        assertEquals("conv-boot", state.conversationId());
        assertEquals("resp-boot", state.responseId());

        JsonNode payload = objectMapper.readTree(server.takeRequest().getBody().readUtf8());
        assertEquals("base coach prompt", payload.path("instructions").asText());
    }

    @Test
    void generate_mapsStructuredResponse() throws Exception {
        enqueueResponsesResponse(minimalStructuredResponse(), "resp-1", "conv-1");

        OpenAiLlmClient client = client();
        LlmGenerateResult result = client.generate(
                "api-key",
                "gpt-4o-mini",
                "sys",
                "user",
                LlmConversationState.empty(),
                LlmPromptContext.empty()
        );

        assertEquals("요약", result.feedback().getSummary());
        assertEquals("시제가 흔들림", result.feedback().getCorrections().grammar().issue());
        assertEquals("Well", result.feedback().getRecommendations().filler().term());
        assertTrue(result.schemaFallbackReasons().isEmpty());
        assertEquals("conv-1", result.conversationState().conversationId());
        assertEquals("resp-1", result.conversationState().responseId());

        RecordedRequest request = server.takeRequest();
        assertEquals("POST", request.getMethod());
        assertEquals("/v1/responses", request.getPath());
        assertEquals("Bearer api-key", request.getHeader("Authorization"));
    }

    @Test
    void generate_reusesConversationWhenProvided() throws Exception {
        enqueueResponsesResponse(minimalStructuredResponse(), "resp-2", "conv-1");

        OpenAiLlmClient client = client();
        client.generate(
                "api-key",
                "gpt-4o-mini",
                "sys",
                "user",
                new LlmConversationState("conv-1", "resp-1", 1),
                LlmPromptContext.empty()
        );

        JsonNode payload = objectMapper.readTree(server.takeRequest().getBody().readUtf8());
        assertEquals("conv-1", payload.path("conversation").asText());
        assertEquals("resp-1", payload.path("previous_response_id").asText());
    }

    @Test
    void generate_retriesAndFallsBackWhenStructuredConversionFails() throws Exception {
        enqueueResponsesResponse("not-json", "resp-1", "conv-1");
        enqueueResponsesResponse("still-not-json", "resp-2", "conv-1");

        OpenAiLlmClient client = client();
        LlmGenerateResult result = client.generate(
                "api-key",
                "gpt-4o-mini",
                "sys",
                "user",
                LlmConversationState.empty(),
                LlmPromptContext.empty()
        );

        assertTrue(result.schemaFallbackReasons().contains("structured_output_conversion_failed"));
        assertTrue(result.schemaFallbackReasons().contains("fallback_parse_failed"));
    }

    @Test
    void generate_sendsTemperatureForLegacyModels() throws Exception {
        enqueueResponsesResponse(minimalStructuredResponse(), "resp-1", "conv-1");

        OpenAiLlmClient client = client();
        client.generate(
                "api-key",
                "gpt-4o-mini",
                "sys",
                "user",
                LlmConversationState.empty(),
                LlmPromptContext.empty()
        );

        JsonNode payload = objectMapper.readTree(server.takeRequest().getBody().readUtf8());
        assertEquals("gpt-4o-mini", payload.path("model").asText());
        assertTrue(payload.has("temperature"));
        assertEquals(0.2, payload.path("temperature").asDouble());
    }

    @Test
    void generate_doesNotSendTemperatureForGpt5Family() throws Exception {
        enqueueResponsesResponse(minimalStructuredResponse(), "resp-1", "conv-1");

        OpenAiLlmClient client = client();
        client.generate(
                "api-key",
                "gpt-5-mini",
                "sys",
                "user",
                LlmConversationState.empty(),
                LlmPromptContext.empty()
        );

        JsonNode payload = objectMapper.readTree(server.takeRequest().getBody().readUtf8());
        assertEquals("gpt-5-mini", payload.path("model").asText());
        assertFalse(payload.has("temperature"));
    }

    @Test
    void generate_includesRecentRecommendationHistoryInMergedPrompt() throws Exception {
        enqueueResponsesResponse(minimalStructuredResponse(), "resp-1", "conv-1");

        OpenAiLlmClient client = client();
        client.generate(
                "api-key",
                "gpt-4o-mini",
                "sys",
                "user",
                LlmConversationState.empty(),
                new LlmPromptContext(
                        "",
                        List.of(),
                        List.of(new LlmPromptContext.RecommendationRecord("well", "vivid", "definitely"))
                )
        );

        JsonNode payload = objectMapper.readTree(server.takeRequest().getBody().readUtf8());
        String mergedPrompt = payload.path("input").path(0).path("content").path(0).path("text").asText();
        assertTrue(mergedPrompt.contains("Recent recommendation terms"));
        assertTrue(mergedPrompt.contains("Filler: well"));
        assertTrue(mergedPrompt.contains("# Current input"));
    }

    private OpenAiLlmClient client() {
        AppProperties properties = new AppProperties();
        properties.getIntegrations().getOpenai().setBaseUrl(server.url("/").toString());
        properties.getIntegrations().getOpenai().setResponsesEnabled(true);
        properties.getIntegrations().getOpenai().getLogging().setResponsePreviewChars(512);
        properties.getIntegrations().getOpenai().getLogging().setFullBody(false);
        properties.getIntegrations().getOpenai().getLogging().setLogSuccessAtFine(true);
        return new OpenAiLlmClient(properties);
    }

    private void enqueueResponsesResponse(String outputText, String responseId, String conversationId) {
        server.enqueue(new MockResponse()
                .setHeader("Content-Type", "application/json")
                .setBody("""
                        {
                          "id":"%s",
                          "conversation":"%s",
                          "output_text":%s
                        }
                        """.formatted(responseId, conversationId, jsonString(outputText))));
    }

    private String jsonString(String text) {
        String escaped = text.replace("\\", "\\\\").replace("\"", "\\\"").replace("\n", "\\n");
        return "\"" + escaped + "\"";
    }

    private String minimalStructuredResponse() {
        return """
                {
                  "summary":"요약",
                  "corrections":{
                    "grammar":{"issue":"시제가 흔들림","fix":"과거형으로 통일"},
                    "expression":{"issue":"어휘가 단조로움","fix":"구체 동사 사용"},
                    "logic":{"issue":"근거가 부족함","fix":"원인/결과 한 문장 추가"}
                  },
                  "recommendations":{
                    "filler":{"term":"Well","usage":"답변 시작 완충"},
                    "adjective":{"term":"vivid","usage":"경험 묘사 강화"},
                    "adverb":{"term":"definitely","usage":"확신 강조"}
                  },
                  "exampleAnswer":"example",
                  "rulebookEvidence":["[a.md] evidence"]
                }
                """;
    }
}
