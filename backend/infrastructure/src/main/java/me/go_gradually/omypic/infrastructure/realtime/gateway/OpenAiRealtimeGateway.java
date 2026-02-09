package me.go_gradually.omypic.infrastructure.realtime.gateway;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import me.go_gradually.omypic.application.realtime.model.RealtimeAudioEventListener;
import me.go_gradually.omypic.application.realtime.model.RealtimeAudioOpenCommand;
import me.go_gradually.omypic.application.realtime.model.RealtimeAudioSession;
import me.go_gradually.omypic.application.realtime.port.RealtimeAudioGateway;
import me.go_gradually.omypic.infrastructure.shared.config.AppProperties;
import org.springframework.stereotype.Component;

import java.net.URI;
import java.net.URLEncoder;
import java.net.http.HttpClient;
import java.net.http.WebSocket;
import java.nio.charset.StandardCharsets;
import java.time.Duration;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.CompletionStage;
import java.util.concurrent.atomic.AtomicBoolean;

@Component
public class OpenAiRealtimeGateway implements RealtimeAudioGateway {
    private final AppProperties properties;
    private final HttpClient httpClient;
    private final ObjectMapper objectMapper = new ObjectMapper();

    public OpenAiRealtimeGateway(AppProperties properties) {
        this.properties = properties;
        this.httpClient = HttpClient.newBuilder()
                .connectTimeout(Duration.ofSeconds(10))
                .build();
    }

    @Override
    public RealtimeAudioSession open(RealtimeAudioOpenCommand command, RealtimeAudioEventListener listener) {
        URI realtimeUri = toRealtimeUri(properties.getIntegrations().getOpenai().getBaseUrl(), command.model());
        OpenAiWebSocketListener webSocketListener = new OpenAiWebSocketListener(listener, objectMapper);
        WebSocket webSocket;
        try {
            webSocket = httpClient.newWebSocketBuilder()
                    .header("Authorization", "Bearer " + command.apiKey())
                    .header("OpenAI-Beta", "realtime=v1")
                    .buildAsync(realtimeUri, webSocketListener)
                    .join();
            sendSessionUpdate(webSocket, command.model());
        } catch (Exception e) {
            throw new IllegalStateException("Failed to connect OpenAI realtime", e);
        }
        return new OpenAiRealtimeAudioSession(webSocket, objectMapper);
    }

    private void sendSessionUpdate(WebSocket webSocket, String model) throws Exception {
        Map<String, Object> root = new LinkedHashMap<>();
        root.put("type", "session.update");
        Map<String, Object> session = new LinkedHashMap<>();
        session.put("input_audio_format", "pcm16");
        session.put("turn_detection", Map.of("type", "server_vad"));
        session.put("input_audio_transcription", Map.of("model", model));
        root.put("session", session);
        webSocket.sendText(objectMapper.writeValueAsString(root), true).join();
    }

    private URI toRealtimeUri(String baseUrl, String model) {
        URI baseUri = URI.create(baseUrl);
        String scheme = "https".equalsIgnoreCase(baseUri.getScheme()) ? "wss" : "ws";
        String path = baseUri.getPath() == null ? "" : baseUri.getPath();
        if (path.endsWith("/")) {
            path = path.substring(0, path.length() - 1);
        }
        String realtimePath = path + "/v1/realtime";
        String query = "model=" + URLEncoder.encode(model, StandardCharsets.UTF_8);
        try {
            return new URI(scheme, null, baseUri.getHost(), baseUri.getPort(), realtimePath, query, null);
        } catch (Exception e) {
            throw new IllegalArgumentException("Invalid OpenAI base URL: " + baseUrl, e);
        }
    }

    private static final class OpenAiRealtimeAudioSession implements RealtimeAudioSession {
        private final WebSocket webSocket;
        private final ObjectMapper objectMapper;
        private final AtomicBoolean closed = new AtomicBoolean(false);

        private OpenAiRealtimeAudioSession(WebSocket webSocket, ObjectMapper objectMapper) {
            this.webSocket = webSocket;
            this.objectMapper = objectMapper;
        }

        @Override
        public void appendBase64Audio(String base64Audio) {
            if (closed.get()) {
                return;
            }
            send(Map.of("type", "input_audio_buffer.append", "audio", base64Audio));
        }

        @Override
        public void commit() {
            if (closed.get()) {
                return;
            }
            send(Map.of("type", "input_audio_buffer.commit"));
        }

        @Override
        public void cancelResponse() {
            if (closed.get()) {
                return;
            }
            send(Map.of("type", "response.cancel"));
        }

        @Override
        public void close() {
            if (!closed.compareAndSet(false, true)) {
                return;
            }
            try {
                webSocket.sendClose(WebSocket.NORMAL_CLOSURE, "bye").join();
            } catch (Exception ignored) {
            }
        }

        private void send(Map<String, Object> payload) {
            try {
                webSocket.sendText(objectMapper.writeValueAsString(payload), true).join();
            } catch (Exception e) {
                throw new IllegalStateException("Failed to send realtime message", e);
            }
        }
    }

    private static final class OpenAiWebSocketListener implements WebSocket.Listener {
        private final RealtimeAudioEventListener listener;
        private final ObjectMapper objectMapper;
        private final StringBuilder textBuffer = new StringBuilder();

        private OpenAiWebSocketListener(RealtimeAudioEventListener listener, ObjectMapper objectMapper) {
            this.listener = listener;
            this.objectMapper = objectMapper;
        }

        @Override
        public void onOpen(WebSocket webSocket) {
            webSocket.request(1);
        }

        @Override
        public CompletionStage<?> onText(WebSocket webSocket, CharSequence data, boolean last) {
            textBuffer.append(data);
            if (last) {
                String payload = textBuffer.toString();
                textBuffer.setLength(0);
                handleMessage(payload);
            }
            webSocket.request(1);
            return CompletableFuture.completedFuture(null);
        }

        @Override
        public CompletionStage<?> onClose(WebSocket webSocket, int statusCode, String reason) {
            if (statusCode >= 4000) {
                listener.onError("Realtime closed: " + reason);
            }
            return CompletableFuture.completedFuture(null);
        }

        @Override
        public void onError(WebSocket webSocket, Throwable error) {
            listener.onError(error == null ? "Realtime websocket error" : error.getMessage());
        }

        private void handleMessage(String payload) {
            try {
                JsonNode root = objectMapper.readTree(payload);
                String type = root.path("type").asText("");
                if ("error".equals(type)) {
                    JsonNode error = root.path("error");
                    String message = error.path("message").asText(root.path("message").asText("Realtime error"));
                    listener.onError(message);
                    return;
                }

                String delta = extractDelta(type, root);
                if (!delta.isBlank()) {
                    listener.onPartialTranscript(delta);
                }

                String finalText = extractFinal(type, root);
                if (!finalText.isBlank()) {
                    listener.onFinalTranscript(finalText);
                }
            } catch (Exception e) {
                listener.onError("Failed to parse realtime event");
            }
        }

        private String extractDelta(String type, JsonNode root) {
            if ("conversation.item.input_audio_transcription.delta".equals(type)) {
                return root.path("delta").asText("");
            }
            if ("response.audio_transcript.delta".equals(type)) {
                return root.path("delta").asText("");
            }
            if (type.endsWith(".delta")) {
                return root.path("delta").asText("");
            }
            return "";
        }

        private String extractFinal(String type, JsonNode root) {
            if ("conversation.item.input_audio_transcription.completed".equals(type)) {
                return root.path("transcript").asText("");
            }
            if ("response.audio_transcript.done".equals(type)) {
                return root.path("transcript").asText(root.path("text").asText(""));
            }
            if (type.endsWith(".completed") || type.endsWith(".done")) {
                String transcript = root.path("transcript").asText("");
                if (!transcript.isBlank()) {
                    return transcript;
                }
                return root.path("text").asText("");
            }
            return "";
        }
    }
}
