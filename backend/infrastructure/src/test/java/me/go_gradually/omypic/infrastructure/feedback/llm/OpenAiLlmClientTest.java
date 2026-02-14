package me.go_gradually.omypic.infrastructure.feedback.llm;

import me.go_gradually.omypic.application.feedback.port.LlmGenerateResult;
import me.go_gradually.omypic.infrastructure.shared.config.AppProperties;
import okhttp3.mockwebserver.MockResponse;
import okhttp3.mockwebserver.MockWebServer;
import okhttp3.mockwebserver.RecordedRequest;
import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

class OpenAiLlmClientTest {

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
    void generate_mapsStructuredResponse() throws Exception {
        enqueueChatResponse("""
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
                """);

        OpenAiLlmClient client = client();
        LlmGenerateResult result = client.generate("api-key", "gpt-4o-mini", "sys", "user");

        assertEquals("요약", result.feedback().getSummary());
        assertEquals("시제가 흔들림", result.feedback().getCorrections().grammar().issue());
        assertEquals("Well", result.feedback().getRecommendations().filler().term());
        assertTrue(result.schemaFallbackReasons().isEmpty());

        RecordedRequest request = server.takeRequest();
        assertEquals("POST", request.getMethod());
        assertEquals("/v1/chat/completions", request.getPath());
        assertEquals("Bearer api-key", request.getHeader("Authorization"));
    }

    @Test
    void generate_retriesAndFallsBackWhenStructuredConversionFails() throws Exception {
        enqueueChatResponse("not-json");
        enqueueChatResponse("still-not-json");

        OpenAiLlmClient client = client();
        LlmGenerateResult result = client.generate("api-key", "gpt-4o-mini", "sys", "user");

        assertTrue(result.schemaFallbackReasons().contains("structured_output_conversion_failed"));
        assertTrue(result.schemaFallbackReasons().contains("fallback_parse_failed"));
    }

    private OpenAiLlmClient client() {
        AppProperties properties = new AppProperties();
        properties.getIntegrations().getOpenai().setBaseUrl(server.url("/").toString());
        properties.getIntegrations().getOpenai().getLogging().setResponsePreviewChars(512);
        properties.getIntegrations().getOpenai().getLogging().setFullBody(false);
        properties.getIntegrations().getOpenai().getLogging().setLogSuccessAtFine(true);
        return new OpenAiLlmClient(properties);
    }

    private void enqueueChatResponse(String content) {
        server.enqueue(new MockResponse()
                .setHeader("Content-Type", "application/json")
                .setBody("""
                        {
                          "id":"chatcmpl-test",
                          "object":"chat.completion",
                          "created":1730000000,
                          "model":"gpt-4o-mini",
                          "choices":[
                            {
                              "index":0,
                              "message":{
                                "role":"assistant",
                                "content":%s
                              },
                              "finish_reason":"stop"
                            }
                          ]
                        }
                        """.formatted(jsonString(content))));
    }

    private String jsonString(String text) {
        String escaped = text.replace("\\", "\\\\").replace("\"", "\\\"").replace("\n", "\\n");
        return "\"" + escaped + "\"";
    }
}
