package me.go_gradually.omypic.domain.wrongnote;

import java.util.UUID;

public record WrongNoteId(String value) {
    public WrongNoteId {
        if (value == null || value.isBlank()) {
            throw new IllegalArgumentException("WrongNoteId is required");
        }
    }

    public static WrongNoteId of(String value) {
        return new WrongNoteId(value);
    }

    public static WrongNoteId newId() {
        return new WrongNoteId(UUID.randomUUID().toString());
    }
}
