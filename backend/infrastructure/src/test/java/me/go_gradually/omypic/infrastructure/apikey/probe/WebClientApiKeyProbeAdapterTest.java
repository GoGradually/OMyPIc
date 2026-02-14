package me.go_gradually.omypic.infrastructure.apikey.probe;

import okhttp3.mockwebserver.MockResponse;
import okhttp3.mockwebserver.MockWebServer;
import okhttp3.mockwebserver.RecordedRequest;
import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.springframework.web.reactive.function.client.WebClient;

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
        server.enqueue(new MockResponse().setResponseCode(200));
        WebClientApiKeyProbeAdapter adapter = adapterWithSingleServer();

        adapter.probe("openai", "sk-test", "gpt-4o-mini");

        RecordedRequest request = server.takeRequest();
        assertEquals("GET", request.getMethod());
        assertEquals("/v1/models", request.getPath());
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

    private WebClientApiKeyProbeAdapter adapterWithSingleServer() {
        String base = server.url("/").toString();
        WebClient webClient = WebClient.builder().baseUrl(base).build();
        return new WebClientApiKeyProbeAdapter(webClient);
    }
}
