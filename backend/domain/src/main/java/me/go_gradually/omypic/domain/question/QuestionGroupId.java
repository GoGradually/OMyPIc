package me.go_gradually.omypic.domain.question;

import java.util.UUID;

public record QuestionGroupId(String value) {
    public QuestionGroupId {
        if (value == null || value.isBlank()) {
            throw new IllegalArgumentException("QuestionGroupId is required");
        }
    }

    public static QuestionGroupId of(String value) {
        return new QuestionGroupId(value);
    }

    public static QuestionGroupId newId() {
        return new QuestionGroupId(UUID.randomUUID().toString());
    }
}
