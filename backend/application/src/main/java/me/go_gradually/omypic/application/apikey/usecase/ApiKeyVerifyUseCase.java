package me.go_gradually.omypic.application.apikey.usecase;

import me.go_gradually.omypic.application.apikey.model.ApiKeyVerifyCommand;
import me.go_gradually.omypic.application.apikey.model.ApiKeyVerifyResult;
import me.go_gradually.omypic.application.apikey.port.ApiKeyProbePort;

import java.util.Locale;

public class ApiKeyVerifyUseCase {
    private final ApiKeyProbePort probePort;

    public ApiKeyVerifyUseCase(ApiKeyProbePort probePort) {
        this.probePort = probePort;
    }

    public ApiKeyVerifyResult verify(ApiKeyVerifyCommand command) {
        String provider = normalizeProvider(command.getProvider());
        String apiKey = command.getApiKey();

        String formatError = validateFormat(provider, apiKey);
        if (formatError != null) {
            return ApiKeyVerifyResult.failure(provider, formatError);
        }

        try {
            probePort.probe(provider, apiKey, command.getModel());
            return ApiKeyVerifyResult.success(provider);
        } catch (Exception e) {
            String message = e.getMessage() == null || e.getMessage().isBlank()
                    ? "Provider verification failed"
                    : e.getMessage();
            return ApiKeyVerifyResult.failure(provider, message);
        }
    }

    private String normalizeProvider(String provider) {
        if (provider == null || provider.isBlank()) {
            throw new IllegalArgumentException("Provider is required");
        }
        return provider.trim().toLowerCase(Locale.ROOT);
    }

    private String validateFormat(String provider, String apiKey) {
        if (apiKey == null || apiKey.isBlank()) {
            return "API key is required";
        }
        return switch (provider) {
            case "openai" -> apiKey.startsWith("sk-") ? null : "OpenAI key must start with sk-";
            case "anthropic" -> apiKey.startsWith("sk-ant-") ? null : "Anthropic key must start with sk-ant-";
            case "gemini" -> apiKey.startsWith("AIza") ? null : "Gemini key must start with AIza";
            default -> "Unsupported provider";
        };
    }
}
