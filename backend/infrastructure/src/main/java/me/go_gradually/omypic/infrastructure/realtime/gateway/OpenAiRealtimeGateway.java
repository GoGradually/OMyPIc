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
    private static final String READ_ALOUD_INSTRUCTION_TEMPLATE = """
You are OMyPIc's strict voice reader.
Read the SCRIPT below verbatim exactly once.
Do not answer, summarize, translate, explain, add, or remove any words.
If the script is a question, read it naturally with interrogative intonation.
Stop immediately after the last character.

SCRIPT:
%s
""";
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
        WebSocket webSocket = connect(command, listener, speechTracker, realtimeUri);
        return new OpenAiRealtimeAudioSession(webSocket, objectMapper, speechTracker);
    }

    private WebSocket connect(RealtimeAudioOpenCommand command,
                              RealtimeAudioEventListener listener,
                              RealtimeSpeechTracker speechTracker,
                              URI realtimeUri) {
        OpenAiWebSocketListener webSocketListener = new OpenAiWebSocketListener(listener, speechTracker, objectMapper);
        try {
            WebSocket webSocket = openWebSocket(command, realtimeUri, webSocketListener);
            sendSessionUpdate(webSocket, command.sttModel());
            return webSocket;
        } catch (Exception e) {
            throw new IllegalStateException("Failed to connect OpenAI realtime", e);
        }
    }

    private WebSocket openWebSocket(RealtimeAudioOpenCommand command,
                                    URI realtimeUri,
                                    OpenAiWebSocketListener webSocketListener) {
        return httpClient.newWebSocketBuilder()
                .header("Authorization", "Bearer " + command.apiKey())
                // 현재 구현은 beta 이벤트 스키마와 함께 동작하도록 유지한다.
                .header("OpenAI-Beta", "realtime=v1")
                .buildAsync(realtimeUri, webSocketListener)
                .join();
    }

    private void sendSessionUpdate(WebSocket webSocket, String model) throws Exception {
        webSocket.sendText(objectMapper.writeValueAsString(sessionUpdatePayload(model)), true).join();
    }

    private Map<String, Object> sessionUpdatePayload(String model) {
        Map<String, Object> root = new LinkedHashMap<>();
        root.put("type", "session.update");
        root.put("session", sessionPayload(model));
        return root;
    }

    private Map<String, Object> sessionPayload(String model) {
        Map<String, Object> session = new LinkedHashMap<>();
        session.put("input_audio_format", "pcm16");
        session.put("turn_detection", turnDetectionPayload());
        session.put("input_audio_transcription", Map.of("model", model));
        return session;
    }

    private Map<String, Object> turnDetectionPayload() {
        // 서버 VAD로 턴 종료를 감지하되 자동 응답 생성은 막고(=false), 기존 응답은 인터럽트 가능하게 둔다.
        return Map.of("type", "server_vad", "create_response", false, "interrupt_response", true);
    }

    private URI toRealtimeUri(String baseUrl, String model) {
        URI baseUri = URI.create(baseUrl);
        String scheme = websocketScheme(baseUri);
        String realtimePath = realtimePath(baseUri);
        String query = "model=" + URLEncoder.encode(model, StandardCharsets.UTF_8);
        try {
            return new URI(scheme, null, baseUri.getHost(), baseUri.getPort(), realtimePath, query, null);
        } catch (Exception e) {
            throw new IllegalArgumentException("Invalid OpenAI base URL: " + baseUrl, e);
        }
    }

    private String websocketScheme(URI baseUri) {
        // OpenAI REST base URL(https://...)을 Realtime websocket URL(wss://.../v1/realtime?model=...)로 변환한다.
        return "https".equalsIgnoreCase(baseUri.getScheme()) ? "wss" : "ws";
    }

    private String realtimePath(URI baseUri) {
        String path = baseUri.getPath() == null ? "" : baseUri.getPath();
        return path.endsWith("/") ? path.substring(0, path.length() - 1) + "/v1/realtime" : path + "/v1/realtime";
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
            root.put("response", responsePayload(turnId, text, voice));
            return root;
        }

        private Map<String, Object> responsePayload(long turnId, String text, String voice) {
            Map<String, Object> response = new LinkedHashMap<>();
            // TTS 전용 응답이므로 대화 히스토리에 누적하지 않도록 conversation을 none으로 고정한다.
            response.put("conversation", "none");
            // Realtime API 제약: audio 단독은 허용되지 않아 ["audio","text"] 조합을 사용한다.
            response.put("modalities", List.of("audio", "text"));
            response.put("voice", resolvedVoice(voice));
            response.put("instructions", readAloudInstruction(text));
            response.put("metadata", metadataPayload(turnId));
            response.put("input", List.of());
            return response;
        }

        private Map<String, Object> metadataPayload(long turnId) {
            // response.created 이후 responseId와 매칭하기 위해 turnId를 metadata에 넣어 둔다.
            return Map.of("turnId", Long.toString(turnId));
        }

        private String readAloudInstruction(String text) {
            return READ_ALOUD_INSTRUCTION_TEMPLATE.formatted(text == null ? "" : text);
        }

        private String resolvedVoice(String voice) {
            return isBlank(voice) ? DEFAULT_TTS_VOICE : voice;
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
                routeEvent(type, root);
            } catch (Exception e) {
                listener.onError("Failed to parse realtime event");
            }
        }

        private void routeEvent(String type, JsonNode root) {
            if (handleStreamingEvent(type, root)) {
                return;
            }
            handleTerminalEvent(type, root);
        }

        private boolean handleStreamingEvent(String type, JsonNode root) {
            if (handleTranscriptDelta(type, root)) {
                return true;
            }
            if (handleTranscriptCompleted(type, root)) {
                return true;
            }
            if (handleResponseCreated(type, root)) {
                return true;
            }
            if (handleAudioDelta(type, root)) {
                return true;
            }
            return handleAudioDone(type, root);
        }

        private void handleTerminalEvent(String type, JsonNode root) {
            if ("response.done".equals(type)) {
                handleResponseDone(root);
            } else if ("error".equals(type)) {
                handleErrorEvent(root);
            }
        }

        private boolean handleTranscriptDelta(String type, JsonNode root) {
            if (!"conversation.item.input_audio_transcription.delta".equals(type)) {
                return false;
            }
            String delta = root.path("delta").asText("");
            if (!delta.isBlank()) {
                listener.onPartialTranscript(delta);
            }
            return true;
        }

        private boolean handleTranscriptCompleted(String type, JsonNode root) {
            if (!"conversation.item.input_audio_transcription.completed".equals(type)) {
                return false;
            }
            String finalText = root.path("transcript").asText("");
            if (!finalText.isBlank()) {
                listener.onFinalTranscript(finalText);
            }
            return true;
        }

        private boolean handleResponseCreated(String type, JsonNode root) {
            if (!"response.created".equals(type)) {
                return false;
            }
            speechTracker.bindResponse(createdEventId(root), createdResponseId(root), root.path("response").path("metadata"));
            return true;
        }

        private boolean handleAudioDelta(String type, JsonNode root) {
            if (!isAudioDeltaEvent(type)) {
                return false;
            }
            speechTracker.emitAudioChunk(extractResponseId(root), root.path("delta").asText(""));
            return true;
        }

        private boolean handleAudioDone(String type, JsonNode root) {
            if (!isAudioDoneEvent(type)) {
                return false;
            }
            speechTracker.completeSpeechByResponseId(extractResponseId(root));
            return true;
        }

        private boolean isAudioDeltaEvent(String type) {
            // GA(output_audio.*)와 beta(audio.*) 이벤트명을 모두 수용한다.
            return "response.output_audio.delta".equals(type) || "response.audio.delta".equals(type);
        }

        private boolean isAudioDoneEvent(String type) {
            return "response.output_audio.done".equals(type) || "response.audio.done".equals(type);
        }

        private String createdResponseId(JsonNode root) {
            return root.path("response").path("id").asText("");
        }

        private String createdEventId(JsonNode root) {
            return root.path("event_id").asText("");
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
            String message = errorMessage(root);
            if (isIgnorableRealtimeError(message)) {
                return;
            }
            boolean matched = failMatchedSpeech(message, errorResponseId(root), errorEventId(root));
            if (!matched) {
                listener.onError(message);
            }
        }

        private String errorMessage(JsonNode root) {
            JsonNode error = root.path("error");
            return error.path("message").asText(root.path("message").asText("Realtime error"));
        }

        private String errorResponseId(JsonNode root) {
            JsonNode error = root.path("error");
            return firstNonBlank(error.path("response_id").asText(""), root.path("response_id").asText(""));
        }

        private String errorEventId(JsonNode root) {
            JsonNode error = root.path("error");
            return firstNonBlank(error.path("event_id").asText(""), root.path("event_id").asText(""));
        }

        private boolean failMatchedSpeech(String message, String responseId, String eventId) {
            if (!isBlank(responseId) && speechTracker.failSpeechByResponseId(responseId, message)) {
                return true;
            }
            return !isBlank(eventId) && speechTracker.failSpeechByEventId(eventId, message);
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
            Long turnId = resolveTurnId(eventId, metadata);
            if (turnId != null) {
                speechTurnByResponseId.put(responseId, turnId);
            }
        }

        private Long resolveTurnId(String eventId, JsonNode metadata) {
            // event_id/metadata(turnId)로 응답과 턴을 매칭해 이후 오디오 chunk를 정확한 턴으로 라우팅한다.
            Long turnId = parseTurnId(metadata);
            if (turnId == null) {
                turnId = fromEventId(eventId);
            }
            if (turnId != null) {
                pendingTurns.removeFirstOccurrence(turnId);
            } else {
                turnId = pendingTurns.pollFirst();
            }
            removeEventId(eventId);
            return turnId;
        }

        private Long fromEventId(String eventId) {
            if (isBlank(eventId)) {
                return null;
            }
            return speechTurnByEventId.remove(eventId);
        }

        private void removeEventId(String eventId) {
            if (!isBlank(eventId)) {
                speechTurnByEventId.remove(eventId);
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
            JsonNode node = metadataTurnNode(metadata);
            if (node.isIntegralNumber()) {
                return node.longValue();
            }
            if (node.isTextual()) {
                return parseLong(node.asText());
            }
            return null;
        }

        private JsonNode metadataTurnNode(JsonNode metadata) {
            // turnId/turn_id 두 표기를 모두 허용해 provider별 metadata 직렬화 차이를 흡수한다.
            JsonNode node = metadata.path("turnId");
            if (node.isMissingNode() || node.isNull()) {
                node = metadata.path("turn_id");
            }
            return node;
        }

        private Long parseLong(String value) {
            try {
                return Long.parseLong(value.trim());
            } catch (NumberFormatException ignored) {
                return null;
            }
        }
    }

    private static boolean isBlank(String value) {
        return value == null || value.isBlank();
    }

    private static String firstNonBlank(String first, String second) {
        return isBlank(first) ? second : first;
    }
}
