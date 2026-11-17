package me.go_gradually.omypic.infrastructure.apikey.probe;

import me.go_gradually.omypic.infrastructure.shared.anthropic.AnthropicClientFactory;
import me.go_gradually.omypic.infrastructure.shared.config.AppProperties;
import okhttp3.mockwebserver.MockResponse;
import okhttp3.mockwebserver.MockWebServer;
import okhttp3.mockwebserver.RecordedRequest;
import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;

class WebClientApiKeyProbeAdapterTest {

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
    void probe_openAi_sendsBearerToken() throws Exception {
        server.enqueue(new MockResponse()
                .setResponseCode(200)
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
                              "message":{"role":"assistant","content":"pong"},
                              "finish_reason":"stop"
                            }
                          ]
                        }
                        """));
        WebClientApiKeyProbeAdapter adapter = adapterWithSingleServer();

        adapter.probe("openai", "sk-test", "gpt-4o-mini");

        RecordedRequest request = server.takeRequest();
        assertEquals("POST", request.getMethod());
        assertEquals("/v1/chat/completions", request.getPath());
        assertEquals("Bearer sk-test", request.getHeader("Authorization"));
    }

    @Test
    void probe_throwsOnUnauthorized() {
        server.enqueue(new MockResponse().setResponseCode(401));
        WebClientApiKeyProbeAdapter adapter = adapterWithSingleServer();

        assertThrows(IllegalStateException.class, () -> adapter.probe("openai", "sk-test", null));
    }

    @Test
    void probe_throwsWhenProviderUnsupported() {
        WebClientApiKeyProbeAdapter adapter = adapterWithSingleServer();
        assertThrows(IllegalArgumentException.class, () -> adapter.probe("gemini", "AIza-test", null));
    }

    @Test
    void probe_anthropic_looksUpModelWithApiKey() throws Exception {
        server.enqueue(new MockResponse()
                .setResponseCode(200)
                .setHeader("Content-Type", "application/json")
                .setBody("""
                        {
                          "id":"claude-opus-5-5",
                          "type":"model",
                          "display_name":"Claude Opus 5.5",
                          "created_at":"2026-01-01T00:00:00Z"
                        }
                        """));
        WebClientApiKeyProbeAdapter adapter = adapterWithSingleServer();

        adapter.probe("anthropic", "sk-ant-test", null);

        RecordedRequest request = server.takeRequest();
        assertEquals("GET", request.getMethod());
        assertEquals("/v1/models/claude-opus-5-5", request.getPath());
        assertEquals("Bearer sk-ant-test", request.getHeader("Authorization"));
    }

    @Test
    void probe_anthropic_throwsOnRejectedKey() {
        server.enqueue(new MockResponse()
                .setResponseCode(401)
                .setHeader("Content-Type", "application/json")
                .setBody("{\"type\":\"error\",\"error\":{\"type\":\"authentication_error\",\"message\":\"invalid x-api-key\"}}"));
        WebClientApiKeyProbeAdapter adapter = adapterWithSingleServer();

        IllegalStateException error = assertThrows(
                IllegalStateException.class,
                () -> adapter.probe("anthropic", "sk-ant-bad", "claude-haiku-5-5")
        );

        assertEquals("Verification failed: the Anthropic API key was rejected", error.getMessage());
    }

    private WebClientApiKeyProbeAdapter adapterWithSingleServer() {
        AppProperties properties = new AppProperties();
        properties.getIntegrations().getOpenai().setBaseUrl(server.url("/").toString());
        properties.getIntegrations().getAnthropic().setBaseUrl(server.url("/").toString());
        properties.getIntegrations().getAnthropic().setMaxRetries(0);
        return new WebClientApiKeyProbeAdapter(properties, new AnthropicClientFactory(properties));
    }
}
