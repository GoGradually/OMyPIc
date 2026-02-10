package me.go_gradually.omypic.presentation.realtime.websocket;

import me.go_gradually.omypic.application.realtime.model.RealtimeSessionUpdateCommand;
import me.go_gradually.omypic.application.realtime.model.RealtimeStartCommand;
import me.go_gradually.omypic.application.realtime.model.RealtimeVoiceSession;
import me.go_gradually.omypic.application.realtime.usecase.RealtimeVoiceUseCase;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.extension.ExtendWith;
import org.mockito.ArgumentCaptor;
import org.mockito.Mock;
import org.mockito.junit.jupiter.MockitoExtension;
import org.springframework.http.HttpHeaders;
import org.springframework.web.socket.CloseStatus;
import org.springframework.web.socket.TextMessage;
import org.springframework.web.socket.WebSocketSession;

import java.net.URI;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.mockito.ArgumentMatchers.any;
import static org.mockito.Mockito.*;

@ExtendWith(MockitoExtension.class)
class RealtimeVoiceWebSocketHandlerTest {

    @Mock
    private RealtimeVoiceUseCase realtimeVoiceUseCase;
    @Mock
    private WebSocketSession webSocketSession;
    @Mock
    private RealtimeVoiceSession realtimeVoiceSession;

    private RealtimeVoiceWebSocketHandler handler;

    @BeforeEach
    void setUp() {
        handler = new RealtimeVoiceWebSocketHandler(realtimeVoiceUseCase);
    }

    @Test
    void afterConnectionEstablished_opensRealtimeSession() throws Exception {
        stubSessionId();
        stubOpenRealtimeSession();
        HttpHeaders headers = new HttpHeaders();
        headers.add("X-API-Key", "api-key");
        when(webSocketSession.getHandshakeHeaders()).thenReturn(headers);
        when(webSocketSession.getUri()).thenReturn(URI.create("ws://localhost/api/realtime/voice?sessionId=s1&conversationModel=gpt-realtime-mini&sttModel=gpt-4o-mini-transcribe"));

        handler.afterConnectionEstablished(webSocketSession);

        ArgumentCaptor<RealtimeStartCommand> captor = ArgumentCaptor.forClass(RealtimeStartCommand.class);
        verify(realtimeVoiceUseCase).open(captor.capture(), any());
        assertEquals("s1", captor.getValue().getSessionId());
        assertEquals("api-key", captor.getValue().getApiKey());
        assertEquals("gpt-realtime-mini", captor.getValue().getConversationModel());
        assertEquals("gpt-4o-mini-transcribe", captor.getValue().getSttModel());
    }

    @Test
    void afterConnectionEstablished_closesWhenMissingCredentials() throws Exception {
        when(webSocketSession.getHandshakeHeaders()).thenReturn(new HttpHeaders());
        when(webSocketSession.getUri()).thenReturn(URI.create("ws://localhost/api/realtime/voice"));

        handler.afterConnectionEstablished(webSocketSession);

        verify(webSocketSession).close(any(CloseStatus.class));
        verify(realtimeVoiceUseCase, never()).open(any(), any());
    }

    @Test
    void afterConnectionEstablished_rejectsApiKeyFromQueryOnly() throws Exception {
        HttpHeaders headers = new HttpHeaders();
        when(webSocketSession.getHandshakeHeaders()).thenReturn(headers);
        when(webSocketSession.getUri()).thenReturn(URI.create("ws://localhost/api/realtime/voice?sessionId=s1&apiKey=query-key"));

        handler.afterConnectionEstablished(webSocketSession);

        verify(webSocketSession).close(any(CloseStatus.class));
        verify(realtimeVoiceUseCase, never()).open(any(), any());
    }

    @Test
    void handleTextMessage_routesAudioAndControlEvents() throws Exception {
        stubSessionId();
        stubOpenRealtimeSession();
        HttpHeaders headers = new HttpHeaders();
        headers.add("X-API-Key", "api-key");
        when(webSocketSession.getHandshakeHeaders()).thenReturn(headers);
        when(webSocketSession.getUri()).thenReturn(URI.create("ws://localhost/api/realtime/voice?sessionId=s1"));
        handler.afterConnectionEstablished(webSocketSession);

        handler.handleTextMessage(webSocketSession, new TextMessage("{\"type\":\"audio.append\",\"data\":{\"audio\":\"abc\"}}"));
        handler.handleTextMessage(webSocketSession, new TextMessage("{\"type\":\"audio.commit\"}"));
        handler.handleTextMessage(webSocketSession, new TextMessage("{\"type\":\"response.cancel\"}"));
        handler.handleTextMessage(webSocketSession, new TextMessage("{\"type\":\"session.update\",\"data\":{\"conversationModel\":\"gpt-realtime\",\"sttModel\":\"gpt-4o-mini-transcribe\",\"feedbackLanguage\":\"en\"}}"));
        handler.handleTextMessage(webSocketSession, new TextMessage("{\"type\":\"session.stop\",\"data\":{\"forced\":true,\"reason\":\"user_stop\"}}"));

        verify(realtimeVoiceSession).appendAudio("abc");
        verify(realtimeVoiceSession).commitAudio();
        verify(realtimeVoiceSession).cancelResponse();
        verify(realtimeVoiceSession).stopSession(true, "user_stop");
        ArgumentCaptor<RealtimeSessionUpdateCommand> captor = ArgumentCaptor.forClass(RealtimeSessionUpdateCommand.class);
        verify(realtimeVoiceSession).update(captor.capture());
        assertEquals("gpt-realtime", captor.getValue().getConversationModel());
        assertEquals("gpt-4o-mini-transcribe", captor.getValue().getSttModel());
        assertEquals("en", captor.getValue().getFeedbackLanguage());
    }

    @Test
    void afterConnectionClosed_closesRealtimeSession() throws Exception {
        stubSessionId();
        stubOpenRealtimeSession();
        HttpHeaders headers = new HttpHeaders();
        headers.add("X-API-Key", "api-key");
        when(webSocketSession.getHandshakeHeaders()).thenReturn(headers);
        when(webSocketSession.getUri()).thenReturn(URI.create("ws://localhost/api/realtime/voice?sessionId=s1"));
        handler.afterConnectionEstablished(webSocketSession);

        handler.afterConnectionClosed(webSocketSession, CloseStatus.NORMAL);

        verify(realtimeVoiceSession).close();
    }

    private void stubSessionId() {
        when(webSocketSession.getId()).thenReturn("ws-1");
    }

    private void stubOpenRealtimeSession() {
        when(realtimeVoiceUseCase.open(any(), any())).thenReturn(realtimeVoiceSession);
    }
}
