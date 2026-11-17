package me.go_gradually.omypic.application.apikey.usecase;

import me.go_gradually.omypic.application.apikey.model.ApiKeyVerifyCommand;
import me.go_gradually.omypic.application.apikey.model.ApiKeyVerifyResult;
import me.go_gradually.omypic.application.apikey.port.ApiKeyProbePort;
import me.go_gradually.omypic.application.feedback.policy.FeedbackModelPolicy;

public class ApiKeyVerifyUseCase {
    private static final String OPENAI_KEY_PREFIX = "sk-";
    private static final String ANTHROPIC_KEY_PREFIX = "sk-ant-";
    private final ApiKeyProbePort probePort;

    public ApiKeyVerifyUseCase(ApiKeyProbePort probePort) {
        this.probePort = probePort;
    }

    public ApiKeyVerifyResult verify(ApiKeyVerifyCommand command) {
        String provider = FeedbackModelPolicy.normalizeProvider(command.getProvider());
        String apiKey = command.getApiKey();
        String formatError = validateFormat(provider, apiKey);
        if (formatError != null) {
            return ApiKeyVerifyResult.failure(provider, formatError);
        }
        FeedbackModelPolicy.validateOrThrow(provider, command.getModel());
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

    private String validateFormat(String provider, String apiKey) {
        if (apiKey == null || apiKey.isBlank()) {
            return "API key is required";
        }
        if (FeedbackModelPolicy.ANTHROPIC_PROVIDER.equals(provider)) {
            return apiKey.startsWith(ANTHROPIC_KEY_PREFIX) ? null : "Anthropic key must start with sk-ant-";
        }
        if (apiKey.startsWith(ANTHROPIC_KEY_PREFIX)) {
            return "This looks like an Anthropic key. Save it as the Claude API key instead.";
        }
        return apiKey.startsWith(OPENAI_KEY_PREFIX) ? null : "OpenAI key must start with sk-";
    }
}
