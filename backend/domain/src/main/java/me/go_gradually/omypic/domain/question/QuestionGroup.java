package me.go_gradually.omypic.domain.question;

import java.util.Objects;

public final class QuestionGroup {
    private final String value;

    private QuestionGroup(String value) {
        this.value = value;
    }

    public static QuestionGroup of(String raw) {
        if (raw == null) {
            throw new IllegalArgumentException("questionGroup must not be blank");
        }
        String normalized = raw.trim();
        if (normalized.isBlank()) {
            throw new IllegalArgumentException("questionGroup must not be blank");
        }
        return new QuestionGroup(normalized);
    }

    public static QuestionGroup fromNullable(String raw) {
        if (raw == null || raw.trim().isBlank()) {
            return null;
        }
        return of(raw);
    }

    public String value() {
        return value;
    }

    @Override
    public boolean equals(Object other) {
        if (this == other) {
            return true;
        }
        if (!(other instanceof QuestionGroup group)) {
            return false;
        }
        return value.equals(group.value);
    }

    @Override
    public int hashCode() {
        return Objects.hash(value);
    }

    @Override
    public String toString() {
        return value;
    }
}
