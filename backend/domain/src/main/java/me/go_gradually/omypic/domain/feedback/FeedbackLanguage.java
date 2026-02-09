package me.go_gradually.omypic.domain.feedback;

import java.util.Locale;

public record FeedbackLanguage(String value) {
    public FeedbackLanguage {
        if (value == null || value.isBlank()) {
            value = "ko";
        }
        String normalized = value.toLowerCase(Locale.ROOT);
        if (!normalized.equals("ko") && !normalized.equals("en")) {
            normalized = "ko";
        }
        value = normalized;
    }

    public static FeedbackLanguage of(String value) {
        return new FeedbackLanguage(value);
    }

    public boolean isEnglish() {
        return "en".equals(value);
    }
}
