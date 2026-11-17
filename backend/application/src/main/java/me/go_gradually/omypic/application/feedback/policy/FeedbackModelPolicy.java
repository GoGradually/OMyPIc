package me.go_gradually.omypic.application.feedback.policy;

import java.util.ArrayList;
import java.util.List;
import java.util.Locale;

public final class FeedbackModelPolicy {
    public static final String OPENAI_PROVIDER = "openai";
    public static final String ANTHROPIC_PROVIDER = "anthropic";

    private static final List<String> OPENAI_MODELS = List.of(
            "gpt-5-mini",
            "gpt-5-nano",
            "gpt-4.1",
            "gpt-4.1-mini",
            "gpt-4.1-nano",
            "gpt-4o",
            "gpt-4o-mini"
    );
    private static final List<String> ANTHROPIC_MODELS = List.of(
            "claude-opus-5-5",
            "claude-sonnet-5-5",
            "claude-haiku-5-5",
            "claude-fable-5-2"
    );
    private static final List<String> PROVIDERS = List.of(OPENAI_PROVIDER, ANTHROPIC_PROVIDER);
    private static final List<String> SUPPORTED_MODELS = allModels();

    private FeedbackModelPolicy() {
    }

    public static void validateOrThrow(String model) {
        if (model == null || model.isBlank()) {
            return;
        }
        if (!SUPPORTED_MODELS.contains(normalize(model))) {
            throw new IllegalArgumentException("Unsupported feedback model: " + model.trim());
        }
    }

    public static void validateOrThrow(String provider, String model) {
        validateOrThrow(model);
        if (model == null || model.isBlank() || provider == null || provider.isBlank()) {
            return;
        }
        String expected = normalizeProvider(provider);
        if (!expected.equals(providerOf(model))) {
            throw new IllegalArgumentException(
                    "Feedback model " + model.trim() + " does not belong to provider " + expected
            );
        }
    }

    public static List<String> supportedModels() {
        return SUPPORTED_MODELS;
    }

    public static List<String> supportedModels(String provider) {
        return ANTHROPIC_PROVIDER.equals(normalizeProvider(provider)) ? ANTHROPIC_MODELS : OPENAI_MODELS;
    }

    public static List<String> supportedProviders() {
        return PROVIDERS;
    }

    public static String defaultModel(String provider) {
        return supportedModels(provider).get(0);
    }

    /**
     * Resolves which provider serves the given model. A blank model falls back to OpenAI,
     * which keeps the server default (voice.feedback-model) behavior unchanged.
     */
    public static String providerOf(String model) {
        if (model == null || model.isBlank()) {
            return OPENAI_PROVIDER;
        }
        return ANTHROPIC_MODELS.contains(normalize(model)) ? ANTHROPIC_PROVIDER : OPENAI_PROVIDER;
    }

    public static String normalizeProvider(String provider) {
        if (provider == null || provider.isBlank()) {
            throw new IllegalArgumentException("Provider is required");
        }
        String normalized = provider.trim().toLowerCase(Locale.ROOT);
        if (!PROVIDERS.contains(normalized)) {
            throw new IllegalArgumentException("Unsupported provider: only openai and anthropic are allowed");
        }
        return normalized;
    }

    private static List<String> allModels() {
        List<String> models = new ArrayList<>(OPENAI_MODELS);
        models.addAll(ANTHROPIC_MODELS);
        return List.copyOf(models);
    }

    private static String normalize(String model) {
        return model.trim().toLowerCase(Locale.ROOT);
    }
}
