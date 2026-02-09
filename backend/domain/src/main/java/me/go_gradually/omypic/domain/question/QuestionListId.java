package me.go_gradually.omypic.domain.question;

import java.util.UUID;

public record QuestionListId(String value) {
    public QuestionListId {
        if (value == null || value.isBlank()) {
            throw new IllegalArgumentException("QuestionListId is required");
        }
    }

    public static QuestionListId of(String value) {
        return new QuestionListId(value);
    }

    public static QuestionListId newId() {
        return new QuestionListId(UUID.randomUUID().toString());
    }
}
