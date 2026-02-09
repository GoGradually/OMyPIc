package me.go_gradually.omypic.presentation.realtime.config;

import me.go_gradually.omypic.presentation.realtime.websocket.RealtimeVoiceWebSocketHandler;
import org.springframework.context.annotation.Configuration;
import org.springframework.web.socket.config.annotation.EnableWebSocket;
import org.springframework.web.socket.config.annotation.WebSocketConfigurer;
import org.springframework.web.socket.config.annotation.WebSocketHandlerRegistry;

@Configuration
@EnableWebSocket
public class RealtimeWebSocketConfig implements WebSocketConfigurer {
    private final RealtimeVoiceWebSocketHandler realtimeVoiceWebSocketHandler;

    public RealtimeWebSocketConfig(RealtimeVoiceWebSocketHandler realtimeVoiceWebSocketHandler) {
        this.realtimeVoiceWebSocketHandler = realtimeVoiceWebSocketHandler;
    }

    @Override
    public void registerWebSocketHandlers(WebSocketHandlerRegistry registry) {
        registry.addHandler(realtimeVoiceWebSocketHandler, "/api/realtime/voice")
                .setAllowedOriginPatterns("*");
    }
}
