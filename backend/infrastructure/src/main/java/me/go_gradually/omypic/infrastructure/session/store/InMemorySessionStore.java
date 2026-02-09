package me.go_gradually.omypic.infrastructure.session.store;

import me.go_gradually.omypic.application.session.port.SessionStorePort;
import me.go_gradually.omypic.domain.session.SessionId;
import me.go_gradually.omypic.domain.session.SessionState;
import org.springframework.stereotype.Component;

import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;

@Component
public class InMemorySessionStore implements SessionStorePort {
    private final Map<SessionId, SessionState> sessions = new ConcurrentHashMap<>();

    @Override
    public SessionState getOrCreate(SessionId sessionId) {
        return sessions.computeIfAbsent(sessionId, SessionState::new);
    }
}
