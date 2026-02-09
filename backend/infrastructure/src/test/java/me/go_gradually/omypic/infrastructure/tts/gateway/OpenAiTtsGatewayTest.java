package me.go_gradually.omypic.infrastructure.tts.gateway;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import me.go_gradually.omypic.application.tts.model.TtsCommand;
import okhttp3.mockwebserver.MockResponse;
import okhttp3.mockwebserver.MockWebServer;
import okhttp3.mockwebserver.RecordedRequest;
import okio.Buffer;
import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.springframework.web.reactive.function.client.WebClient;

import java.io.ByteArrayOutputStream;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

class OpenAiTtsGatewayTest {

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
    void stream_sendsRequestAndYieldsAudioBytes() throws Exception {
        byte[] responseBytes = "mp3-chunk".getBytes();
        server.enqueue(new MockResponse()
                .setHeader("Content-Type", "application/octet-stream")
                .setBody(new Buffer().write(responseBytes)));

        OpenAiTtsGateway gateway = new OpenAiTtsGateway(WebClient.builder().build(), server.url("/v1/audio/speech").toString());

        ByteArrayOutputStream out = new ByteArrayOutputStream();
        for (byte[] chunk : gateway.stream("api-key", new TtsCommand("hello", "alloy"))) {
            out.write(chunk);
        }

        assertEquals("mp3-chunk", out.toString());

        RecordedRequest request = server.takeRequest();
        assertEquals("POST", request.getMethod());
        assertEquals("/v1/audio/speech", request.getPath());
        assertEquals("Bearer api-key", request.getHeader("Authorization"));

        JsonNode payload = objectMapper.readTree(request.getBody().readUtf8());
        assertEquals("gpt-4o-mini-tts", payload.path("model").asText());
        assertEquals("alloy", payload.path("voice").asText());
        assertEquals("hello", payload.path("input").asText());
        assertTrue(payload.path("stream").asBoolean());
    }
}
