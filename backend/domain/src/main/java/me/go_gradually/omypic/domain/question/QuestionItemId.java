package me.go_gradually.omypic.domain.question;

import java.util.UUID;

public record QuestionItemId(String value) {
    public QuestionItemId {
        if (value == null || value.isBlank()) {
            throw new IllegalArgumentException("QuestionItemId is required");
        }
    }

    public static QuestionItemId of(String value) {
        return new QuestionItemId(value);
    }

    public static QuestionItemId newId() {
        return new QuestionItemId(UUID.randomUUID().toString());
    }
}
