package me.go_gradually.omypic.presentation.realtime.config;

import me.go_gradually.omypic.presentation.realtime.websocket.RealtimeVoiceWebSocketHandler;
import org.springframework.context.annotation.Bean;
import org.springframework.context.annotation.Configuration;
import org.springframework.web.socket.config.annotation.EnableWebSocket;
import org.springframework.web.socket.config.annotation.WebSocketConfigurer;
import org.springframework.web.socket.config.annotation.WebSocketHandlerRegistry;
import org.springframework.web.socket.server.standard.ServletServerContainerFactoryBean;

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

    @Bean
    public ServletServerContainerFactoryBean webSocketContainer() {
        ServletServerContainerFactoryBean container = new ServletServerContainerFactoryBean();
        container.setMaxTextMessageBufferSize(1_048_576);
        container.setMaxBinaryMessageBufferSize(1_048_576);
        return container;
    }
}
