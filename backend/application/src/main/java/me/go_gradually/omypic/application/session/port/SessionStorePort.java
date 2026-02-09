package me.go_gradually.omypic.application.session.port;

import me.go_gradually.omypic.domain.session.SessionId;
import me.go_gradually.omypic.domain.session.SessionState;

public interface SessionStorePort {
    SessionState getOrCreate(SessionId sessionId);
}
