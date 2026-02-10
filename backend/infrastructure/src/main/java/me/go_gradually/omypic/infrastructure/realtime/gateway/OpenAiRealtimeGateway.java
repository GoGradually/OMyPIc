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
import java.util.Deque;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.CompletionStage;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.ConcurrentLinkedDeque;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicLong;

@Component
public class OpenAiRealtimeGateway implements RealtimeAudioGateway {
    private static final String DEFAULT_TTS_VOICE = "alloy";
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
        URI realtimeUri = toRealtimeUri(properties.getIntegrations().getOpenai().getBaseUrl(), command.conversationModel());
        RealtimeSpeechTracker speechTracker = new RealtimeSpeechTracker(listener);
        OpenAiWebSocketListener webSocketListener = new OpenAiWebSocketListener(listener, speechTracker, objectMapper);
        WebSocket webSocket;
        try {
            webSocket = httpClient.newWebSocketBuilder()
                    .header("Authorization", "Bearer " + command.apiKey())
                    // 현재 구현은 beta 이벤트 스키마와 함께 동작하도록 유지한다.
                    .header("OpenAI-Beta", "realtime=v1")
                    .buildAsync(realtimeUri, webSocketListener)
                    .join();
            // 연결 직후 session.update를 보내 입력 오디오/전사(VAD 포함) 동작을 명시적으로 초기화한다.
            sendSessionUpdate(webSocket, command.sttModel());
        } catch (Exception e) {
            throw new IllegalStateException("Failed to connect OpenAI realtime", e);
        }
        return new OpenAiRealtimeAudioSession(webSocket, objectMapper, speechTracker);
    }

    private void sendSessionUpdate(WebSocket webSocket, String model) throws Exception {
        Map<String, Object> root = new LinkedHashMap<>();
        root.put("type", "session.update");
        Map<String, Object> session = new LinkedHashMap<>();
        session.put("input_audio_format", "pcm16");
        // 서버 VAD로 턴 종료를 감지하되 자동 응답 생성은 막고(=false), 기존 응답은 인터럽트 가능하게 둔다.
        session.put("turn_detection", Map.of(
                "type", "server_vad",
                "create_response", false,
                "interrupt_response", true
        ));
        session.put("input_audio_transcription", Map.of("model", model));
        root.put("session", session);
        webSocket.sendText(objectMapper.writeValueAsString(root), true).join();
    }

    private URI toRealtimeUri(String baseUrl, String model) {
        URI baseUri = URI.create(baseUrl);
        // OpenAI REST base URL(https://...)을 Realtime websocket URL(wss://.../v1/realtime?model=...)로 변환한다.
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
        private final RealtimeSpeechTracker speechTracker;
        private final AtomicBoolean closed = new AtomicBoolean(false);
        private final AtomicLong eventSequence = new AtomicLong(0);

        private OpenAiRealtimeAudioSession(WebSocket webSocket,
                                           ObjectMapper objectMapper,
                                           RealtimeSpeechTracker speechTracker) {
            this.webSocket = webSocket;
            this.objectMapper = objectMapper;
            this.speechTracker = speechTracker;
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
        public void speakText(long turnId, String text, String voice) {
            if (closed.get() || isBlank(text)) {
                return;
            }
            String eventId = "tts-" + eventSequence.incrementAndGet();
            speechTracker.registerSpeech(eventId, turnId);
            try {
                send(speechPayload(eventId, turnId, text, voice));
            } catch (RuntimeException e) {
                speechTracker.discardSpeechByEventId(eventId);
                throw e;
            }
        }

        @Override
        public void close() {
            if (!closed.compareAndSet(false, true)) {
                return;
            }
            speechTracker.failAll("Realtime session closed");
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

        private Map<String, Object> speechPayload(String eventId, long turnId, String text, String voice) {
            Map<String, Object> root = new LinkedHashMap<>();
            root.put("type", "response.create");
            root.put("event_id", eventId);

            Map<String, Object> response = new LinkedHashMap<>();
            // TTS 전용 응답이므로 대화 히스토리에 누적하지 않도록 conversation을 none으로 고정한다.
            response.put("conversation", "none");
            response.put("output_modalities", List.of("audio"));
            response.put("instructions", "Read the provided text exactly as written. Do not add or remove words.");
            // response.created 이후 responseId와 매칭하기 위해 turnId를 metadata에 넣어 둔다.
            response.put("metadata", Map.of("turnId", Long.toString(turnId)));
            response.put("input", List.of(Map.of(
                    "type", "message",
                    "role", "user",
                    "content", List.of(Map.of("type", "input_text", "text", text))
            )));
            response.put("audio", Map.of(
                    "output", Map.of(
                            "voice", isBlank(voice) ? DEFAULT_TTS_VOICE : voice,
                            "format", Map.of("type", "audio/pcm")
                    )
            ));

            root.put("response", response);
            return root;
        }
    }

    private static final class OpenAiWebSocketListener implements WebSocket.Listener {
        private final RealtimeAudioEventListener listener;
        private final RealtimeSpeechTracker speechTracker;
        private final ObjectMapper objectMapper;
        private final StringBuilder textBuffer = new StringBuilder();

        private OpenAiWebSocketListener(RealtimeAudioEventListener listener,
                                        RealtimeSpeechTracker speechTracker,
                                        ObjectMapper objectMapper) {
            this.listener = listener;
            this.speechTracker = speechTracker;
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
            speechTracker.failAll("Realtime websocket closed");
            if (statusCode >= 4000) {
                listener.onError("Realtime closed: " + reason);
            }
            return CompletableFuture.completedFuture(null);
        }

        @Override
        public void onError(WebSocket webSocket, Throwable error) {
            speechTracker.failAll("Realtime websocket error");
            listener.onError(error == null ? "Realtime websocket error" : error.getMessage());
        }

        private void handleMessage(String payload) {
            try {
                JsonNode root = objectMapper.readTree(payload);
                String type = root.path("type").asText("");
                switch (type) {
                    case "conversation.item.input_audio_transcription.delta" -> {
                        String delta = root.path("delta").asText("");
                        if (!delta.isBlank()) {
                            listener.onPartialTranscript(delta);
                        }
                    }
                    case "conversation.item.input_audio_transcription.completed" -> {
                        String finalText = root.path("transcript").asText("");
                        if (!finalText.isBlank()) {
                            listener.onFinalTranscript(finalText);
                        }
                    }
                    case "response.created" -> {
                        String responseId = root.path("response").path("id").asText("");
                        String eventId = root.path("event_id").asText("");
                        JsonNode metadata = root.path("response").path("metadata");
                        speechTracker.bindResponse(eventId, responseId, metadata);
                    }
                    // GA(output_audio.*)와 beta(audio.*) 이벤트명을 모두 수용한다.
                    case "response.output_audio.delta", "response.audio.delta" -> {
                        String responseId = extractResponseId(root);
                        String delta = root.path("delta").asText("");
                        speechTracker.emitAudioChunk(responseId, delta);
                    }
                    case "response.output_audio.done", "response.audio.done" -> {
                        speechTracker.completeSpeechByResponseId(extractResponseId(root));
                    }
                    case "response.done" -> handleResponseDone(root);
                    case "error" -> handleErrorEvent(root);
                    default -> {
                        // Ignore other realtime events.
                    }
                }
            } catch (Exception e) {
                listener.onError("Failed to parse realtime event");
            }
        }

        private void handleResponseDone(JsonNode root) {
            String responseId = extractResponseId(root);
            JsonNode response = root.path("response");
            String status = response.path("status").asText("");
            if ("failed".equalsIgnoreCase(status)) {
                String message = response.path("status_details").path("error").path("message")
                        .asText("Realtime response failed");
                speechTracker.failSpeechByResponseId(responseId, message);
                return;
            }
            speechTracker.completeSpeechByResponseId(responseId);
        }

        private void handleErrorEvent(JsonNode root) {
            JsonNode error = root.path("error");
            String message = error.path("message").asText(root.path("message").asText("Realtime error"));
            if (isIgnorableRealtimeError(message)) {
                return;
            }
            String responseId = firstNonBlank(error.path("response_id").asText(""), root.path("response_id").asText(""));
            String eventId = firstNonBlank(error.path("event_id").asText(""), root.path("event_id").asText(""));
            boolean matched = false;
            if (!isBlank(responseId)) {
                matched = speechTracker.failSpeechByResponseId(responseId, message);
            }
            if (!matched && !isBlank(eventId)) {
                matched = speechTracker.failSpeechByEventId(eventId, message);
            }
            if (!matched) {
                listener.onError(message);
            }
        }

        private String extractResponseId(JsonNode root) {
            return firstNonBlank(root.path("response_id").asText(""), root.path("response").path("id").asText(""));
        }

        private boolean isIgnorableRealtimeError(String message) {
            if (message == null || message.isBlank()) {
                return false;
            }
            String normalized = message.toLowerCase(Locale.ROOT);
            // cancel 타이밍 경합이나 매우 짧은 버퍼 입력은 사용자 체감 오류가 아니어서 로그성 이벤트로만 무시한다.
            if (normalized.contains("cancellation failed") && normalized.contains("no active response found")) {
                return true;
            }
            return normalized.contains("input audio buffer") && normalized.contains("buffer too small");
        }
    }

    private static final class RealtimeSpeechTracker {
        private final RealtimeAudioEventListener listener;
        private final Map<String, Long> speechTurnByEventId = new ConcurrentHashMap<>();
        private final Map<String, Long> speechTurnByResponseId = new ConcurrentHashMap<>();
        private final Deque<Long> pendingTurns = new ConcurrentLinkedDeque<>();
        private final Set<String> completedResponseIds = ConcurrentHashMap.newKeySet();

        private RealtimeSpeechTracker(RealtimeAudioEventListener listener) {
            this.listener = listener;
        }

        private void registerSpeech(String eventId, long turnId) {
            speechTurnByEventId.put(eventId, turnId);
            pendingTurns.addLast(turnId);
        }

        private void bindResponse(String eventId, String responseId, JsonNode metadata) {
            if (isBlank(responseId)) {
                return;
            }
            // event_id/metadata(turnId)로 응답과 턴을 매칭해 이후 오디오 chunk를 정확한 턴으로 라우팅한다.
            Long turnId = parseTurnId(metadata);
            if (turnId == null && !isBlank(eventId)) {
                turnId = speechTurnByEventId.remove(eventId);
            }
            if (turnId != null) {
                pendingTurns.removeFirstOccurrence(turnId);
            } else {
                turnId = pendingTurns.pollFirst();
            }
            if (!isBlank(eventId)) {
                speechTurnByEventId.remove(eventId);
            }
            if (turnId != null) {
                speechTurnByResponseId.put(responseId, turnId);
            }
        }

        private void emitAudioChunk(String responseId, String base64Audio) {
            if (isBlank(responseId) || isBlank(base64Audio)) {
                return;
            }
            Long turnId = speechTurnByResponseId.get(responseId);
            if (turnId != null) {
                listener.onAssistantAudioChunk(turnId, base64Audio);
            }
        }

        private void completeSpeechByResponseId(String responseId) {
            if (isBlank(responseId) || !completedResponseIds.add(responseId)) {
                return;
            }
            Long turnId = speechTurnByResponseId.remove(responseId);
            if (turnId != null) {
                listener.onAssistantAudioCompleted(turnId);
            }
        }

        private boolean failSpeechByResponseId(String responseId, String message) {
            if (isBlank(responseId)) {
                return false;
            }
            completedResponseIds.add(responseId);
            Long turnId = speechTurnByResponseId.remove(responseId);
            if (turnId == null) {
                return false;
            }
            listener.onAssistantAudioFailed(turnId, message);
            return true;
        }

        private boolean failSpeechByEventId(String eventId, String message) {
            if (isBlank(eventId)) {
                return false;
            }
            Long turnId = speechTurnByEventId.remove(eventId);
            if (turnId == null) {
                return false;
            }
            pendingTurns.removeFirstOccurrence(turnId);
            listener.onAssistantAudioFailed(turnId, message);
            return true;
        }

        private void failAll(String message) {
            for (Long turnId : speechTurnByResponseId.values()) {
                listener.onAssistantAudioFailed(turnId, message);
            }
            for (Long turnId : speechTurnByEventId.values()) {
                listener.onAssistantAudioFailed(turnId, message);
            }
            speechTurnByResponseId.clear();
            speechTurnByEventId.clear();
            pendingTurns.clear();
            completedResponseIds.clear();
        }

        private void discardSpeechByEventId(String eventId) {
            if (isBlank(eventId)) {
                return;
            }
            Long turnId = speechTurnByEventId.remove(eventId);
            if (turnId != null) {
                pendingTurns.removeFirstOccurrence(turnId);
            }
        }

        private Long parseTurnId(JsonNode metadata) {
            if (metadata == null || metadata.isMissingNode() || metadata.isNull()) {
                return null;
            }
            // turnId/turn_id 두 표기를 모두 허용해 provider별 metadata 직렬화 차이를 흡수한다.
            JsonNode node = metadata.path("turnId");
            if (node.isMissingNode() || node.isNull()) {
                node = metadata.path("turn_id");
            }
            if (node.isIntegralNumber()) {
                return node.longValue();
            }
            if (node.isTextual()) {
                try {
                    return Long.parseLong(node.asText().trim());
                } catch (NumberFormatException ignored) {
                    return null;
                }
            }
            return null;
        }
    }

    private static boolean isBlank(String value) {
        return value == null || value.isBlank();
    }

    private static String firstNonBlank(String first, String second) {
        return isBlank(first) ? second : first;
    }
}
