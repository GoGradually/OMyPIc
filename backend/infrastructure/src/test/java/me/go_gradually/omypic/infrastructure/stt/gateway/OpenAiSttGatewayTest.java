package me.go_gradually.omypic.infrastructure.stt.gateway;

import me.go_gradually.omypic.application.stt.model.VadSettings;
import okhttp3.mockwebserver.MockResponse;
import okhttp3.mockwebserver.MockWebServer;
import okhttp3.mockwebserver.RecordedRequest;
import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.springframework.web.reactive.function.client.WebClient;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

class OpenAiSttGatewayTest {

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
    void transcribe_callsTranscriptionsEndpoint_andParsesText() throws Exception {
        server.enqueue(new MockResponse()
                .setHeader("Content-Type", "application/json")
                .setBody("{\"text\":\"hello\"}"));

        OpenAiSttGateway gateway = new OpenAiSttGateway(WebClient.builder()
                .baseUrl(server.url("/").toString())
                .build());

        String text = gateway.transcribe(new byte[]{1, 2, 3}, "whisper-1", "api-key", false, new VadSettings(111, 222, 0.7));

        assertEquals("hello", text);

        RecordedRequest request = server.takeRequest();
        assertEquals("POST", request.getMethod());
        assertEquals("/v1/audio/transcriptions", request.getPath());
        assertEquals("Bearer api-key", request.getHeader("Authorization"));
        assertTrue(request.getHeader("Content-Type").contains("multipart/form-data"));
        String body = request.getBody().readUtf8();
        assertTrue(body.contains("name=\"model\""));
        assertTrue(body.contains("whisper-1"));
        assertTrue(body.contains("name=\"prefix_padding_ms\""));
        assertTrue(body.contains("111"));
    }

    @Test
    void transcribe_callsTranslationsEndpoint_whenTranslateTrue() throws Exception {
        server.enqueue(new MockResponse()
                .setHeader("Content-Type", "application/json")
                .setBody("{\"text\":\"translated\"}"));

        OpenAiSttGateway gateway = new OpenAiSttGateway(WebClient.builder()
                .baseUrl(server.url("/").toString())
                .build());

        String text = gateway.transcribe(new byte[]{9}, "whisper-1", "api-key", true, new VadSettings(1, 2, 0.5));

        assertEquals("translated", text);
        assertEquals("/v1/audio/translations", server.takeRequest().getPath());
    }

    @Test
    void transcribe_returnsEmptyString_whenTextMissing() throws Exception {
        server.enqueue(new MockResponse()
                .setHeader("Content-Type", "application/json")
                .setBody("{}"));

        OpenAiSttGateway gateway = new OpenAiSttGateway(WebClient.builder()
                .baseUrl(server.url("/").toString())
                .build());

        String text = gateway.transcribe(new byte[]{1}, "whisper-1", "api-key", false, new VadSettings(1, 2, 0.5));

        assertEquals("", text);
    }
}
