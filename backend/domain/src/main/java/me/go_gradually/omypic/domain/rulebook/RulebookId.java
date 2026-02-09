package me.go_gradually.omypic.domain.rulebook;

import java.util.UUID;

public record RulebookId(String value) {
    public RulebookId {
        if (value == null || value.isBlank()) {
            throw new IllegalArgumentException("RulebookId is required");
        }
    }

    public static RulebookId of(String value) {
        return new RulebookId(value);
    }

    public static RulebookId newId() {
        return new RulebookId(UUID.randomUUID().toString());
    }
}
