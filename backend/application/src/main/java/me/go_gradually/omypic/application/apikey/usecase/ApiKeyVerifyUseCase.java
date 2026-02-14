package me.go_gradually.omypic.application.apikey.usecase;

import me.go_gradually.omypic.application.apikey.model.ApiKeyVerifyCommand;
import me.go_gradually.omypic.application.apikey.model.ApiKeyVerifyResult;
import me.go_gradually.omypic.application.apikey.port.ApiKeyProbePort;
import me.go_gradually.omypic.application.feedback.policy.FeedbackModelPolicy;

import java.util.Locale;

public class ApiKeyVerifyUseCase {
    private static final String OPENAI_PROVIDER = "openai";
    private final ApiKeyProbePort probePort;

    public ApiKeyVerifyUseCase(ApiKeyProbePort probePort) {
        this.probePort = probePort;
    }

    public ApiKeyVerifyResult verify(ApiKeyVerifyCommand command) {
        String provider = normalizeProvider(command.getProvider());
        String apiKey = command.getApiKey();
        String formatError = validateFormat(apiKey);
        if (formatError != null) {
            return ApiKeyVerifyResult.failure(provider, formatError);
        }
        FeedbackModelPolicy.validateOrThrow(command.getModel());
        return verifyProvider(provider, apiKey, command.getModel());
    }

    private ApiKeyVerifyResult verifyProvider(String provider, String apiKey, String model) {
        try {
            probePort.probe(provider, apiKey, model);
            return ApiKeyVerifyResult.success(provider);
        } catch (Exception e) {
            return ApiKeyVerifyResult.failure(provider, failureMessage(e));
        }
    }

    private String failureMessage(Exception e) {
        String message = e.getMessage();
        return message == null || message.isBlank() ? "Provider verification failed" : message;
    }

    private String normalizeProvider(String provider) {
        if (provider == null || provider.isBlank()) {
            throw new IllegalArgumentException("Provider is required");
        }
        String normalized = provider.trim().toLowerCase(Locale.ROOT);
        if (!OPENAI_PROVIDER.equals(normalized)) {
            throw new IllegalArgumentException("Unsupported provider: only openai is allowed");
        }
        return normalized;
    }

    private String validateFormat(String apiKey) {
        if (apiKey == null || apiKey.isBlank()) {
            return "API key is required";
        }
        return apiKey.startsWith("sk-") ? null : "OpenAI key must start with sk-";
    }
}
