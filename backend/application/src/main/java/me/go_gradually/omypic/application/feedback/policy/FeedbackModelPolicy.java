package me.go_gradually.omypic.application.feedback.policy;

import java.util.Locale;
import java.util.Set;

public final class FeedbackModelPolicy {
    private static final Set<String> SUPPORTED_MODELS = Set.of(
            "gpt-5-mini",
            "gpt-5-nano",
            "gpt-4.1",
            "gpt-4.1-mini",
            "gpt-4.1-nano",
            "gpt-4o",
            "gpt-4o-mini"
    );

    private FeedbackModelPolicy() {
    }

    public static void validateOrThrow(String model) {
        if (model == null || model.isBlank()) {
            return;
        }
        String normalized = normalize(model);
        if (!SUPPORTED_MODELS.contains(normalized)) {
            throw new IllegalArgumentException("Unsupported feedback model: " + model.trim());
        }
    }

    private static String normalize(String model) {
        return model.trim().toLowerCase(Locale.ROOT);
    }
}
