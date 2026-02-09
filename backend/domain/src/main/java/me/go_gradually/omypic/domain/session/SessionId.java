package me.go_gradually.omypic.domain.session;

public record SessionId(String value) {
    public SessionId {
        if (value == null || value.isBlank()) {
            throw new IllegalArgumentException("SessionId is required");
        }
    }

    public static SessionId of(String value) {
        return new SessionId(value);
    }
}
