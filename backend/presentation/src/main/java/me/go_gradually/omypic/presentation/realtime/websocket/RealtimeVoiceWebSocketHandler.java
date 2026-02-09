package me.go_gradually.omypic.presentation.realtime.websocket;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import me.go_gradually.omypic.application.realtime.model.RealtimeSessionUpdateCommand;
import me.go_gradually.omypic.application.realtime.model.RealtimeStartCommand;
import me.go_gradually.omypic.application.realtime.model.RealtimeVoiceSession;
import me.go_gradually.omypic.application.realtime.usecase.RealtimeVoiceUseCase;
import org.springframework.stereotype.Component;
import org.springframework.web.socket.CloseStatus;
import org.springframework.web.socket.TextMessage;
import org.springframework.web.socket.WebSocketSession;
import org.springframework.web.socket.handler.ConcurrentWebSocketSessionDecorator;
import org.springframework.web.socket.handler.TextWebSocketHandler;

import java.net.URI;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;

@Component
public class RealtimeVoiceWebSocketHandler extends TextWebSocketHandler {
    private final RealtimeVoiceUseCase realtimeVoiceUseCase;
    private final ObjectMapper objectMapper = new ObjectMapper();
    private final Map<String, RealtimeVoiceSession> sessionBySocketId = new ConcurrentHashMap<>();

    public RealtimeVoiceWebSocketHandler(RealtimeVoiceUseCase realtimeVoiceUseCase) {
        this.realtimeVoiceUseCase = realtimeVoiceUseCase;
    }

    @Override
    public void afterConnectionEstablished(WebSocketSession rawSession) throws Exception {
        WebSocketSession session = new ConcurrentWebSocketSessionDecorator(rawSession, 10_000, 1_048_576);
        RealtimeStartCommand command = buildStartCommand(session);
        if (command == null) {
            session.close(CloseStatus.BAD_DATA.withReason("sessionId and apiKey are required"));
            return;
        }

        RealtimeVoiceSession voiceSession = realtimeVoiceUseCase.open(command, (event, payload) -> sendEvent(session, event, payload));
        sessionBySocketId.put(session.getId(), voiceSession);
    }

    @Override
    protected void handleTextMessage(WebSocketSession session, TextMessage message) throws Exception {
        RealtimeVoiceSession realtimeSession = sessionBySocketId.get(session.getId());
        if (realtimeSession == null) {
            sendEvent(session, "error", Map.of("message", "Unknown realtime session"));
            return;
        }

        JsonNode root = objectMapper.readTree(message.getPayload());
        String type = root.path("type").asText("");
        JsonNode data = root.has("data") ? root.path("data") : root;

        switch (type) {
            case "audio.append" -> realtimeSession.appendAudio(readString(data, "audio"));
            case "audio.commit" -> realtimeSession.commitAudio();
            case "response.cancel" -> realtimeSession.cancelResponse();
            case "session.update" -> realtimeSession.update(toUpdateCommand(data));
            default -> sendEvent(session, "error", Map.of("message", "Unsupported event type: " + type));
        }
    }

    @Override
    public void handleTransportError(WebSocketSession session, Throwable exception) throws Exception {
        sendEvent(session, "error", Map.of("message", exception == null ? "Transport error" : exception.getMessage()));
        RealtimeVoiceSession realtimeSession = sessionBySocketId.remove(session.getId());
        if (realtimeSession != null) {
            realtimeSession.close();
        }
    }

    @Override
    public void afterConnectionClosed(WebSocketSession session, CloseStatus status) throws Exception {
        RealtimeVoiceSession realtimeSession = sessionBySocketId.remove(session.getId());
        if (realtimeSession != null) {
            realtimeSession.close();
        }
    }

    private RealtimeStartCommand buildStartCommand(WebSocketSession session) {
        String sessionId = firstNonBlank(readQuery(session.getUri(), "sessionId"), readHeader(session, "X-Session-Id"));
        String apiKey = readHeader(session, "X-API-Key");
        if (isBlank(sessionId) || isBlank(apiKey)) {
            return null;
        }
        RealtimeStartCommand command = new RealtimeStartCommand();
        command.setSessionId(sessionId);
        command.setApiKey(apiKey);
        command.setConversationModel(readQuery(session.getUri(), "conversationModel"));
        command.setSttModel(readQuery(session.getUri(), "sttModel"));
        return command;
    }

    private RealtimeSessionUpdateCommand toUpdateCommand(JsonNode data) {
        RealtimeSessionUpdateCommand command = new RealtimeSessionUpdateCommand();
        command.setConversationModel(readString(data, "conversationModel"));
        command.setSttModel(readString(data, "sttModel"));
        command.setFeedbackProvider(readString(data, "feedbackProvider"));
        command.setFeedbackModel(readString(data, "feedbackModel"));
        command.setFeedbackApiKey(readString(data, "feedbackApiKey"));
        command.setFeedbackLanguage(readString(data, "feedbackLanguage"));
        command.setTtsVoice(readString(data, "ttsVoice"));
        return command;
    }

    private boolean sendEvent(WebSocketSession session, String event, Object payload) {
        if (!session.isOpen()) {
            return false;
        }
        try {
            Map<String, Object> envelope = new LinkedHashMap<>();
            envelope.put("type", event);
            envelope.put("data", payload);
            session.sendMessage(new TextMessage(objectMapper.writeValueAsString(envelope)));
            return true;
        } catch (Exception e) {
            return false;
        }
    }

    private String readString(JsonNode node, String field) {
        JsonNode value = node.path(field);
        return value.isTextual() ? value.asText() : null;
    }

    private String readHeader(WebSocketSession session, String headerName) {
        return session.getHandshakeHeaders().getFirst(headerName);
    }

    private String readQuery(URI uri, String key) {
        if (uri == null || isBlank(uri.getQuery())) {
            return null;
        }
        String[] pairs = uri.getQuery().split("&");
        for (String pair : pairs) {
            String[] kv = pair.split("=", 2);
            if (kv.length == 2 && key.equals(kv[0])) {
                return kv[1];
            }
        }
        return null;
    }

    private String firstNonBlank(String first, String second) {
        if (!isBlank(first)) {
            return first;
        }
        return second;
    }

    private boolean isBlank(String value) {
        return value == null || value.isBlank();
    }
}
