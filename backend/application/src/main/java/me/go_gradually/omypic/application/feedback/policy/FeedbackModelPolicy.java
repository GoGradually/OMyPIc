package me.go_gradually.omypic.application.feedback.policy;

import java.util.Locale;
import java.util.List;
import java.util.Set;

public final class FeedbackModelPolicy {
    private static final List<String> SUPPORTED_MODELS = List.of(
            "gpt-5-mini",
            "gpt-5-nano",
            "gpt-4.1",
            "gpt-4.1-mini",
            "gpt-4.1-nano",
            "gpt-4o",
            "gpt-4o-mini"
    );
    private static final Set<String> SUPPORTED_MODEL_SET = Set.copyOf(SUPPORTED_MODELS);

    private FeedbackModelPolicy() {
    }

    public static void validateOrThrow(String model) {
        if (model == null || model.isBlank()) {
            return;
        }
        String normalized = normalize(model);
        if (!SUPPORTED_MODEL_SET.contains(normalized)) {
            throw new IllegalArgumentException("Unsupported feedback model: " + model.trim());
        }
    }

    public static List<String> supportedModels() {
        return SUPPORTED_MODELS;
    }

    private static String normalize(String model) {
        return model.trim().toLowerCase(Locale.ROOT);
    }
}
