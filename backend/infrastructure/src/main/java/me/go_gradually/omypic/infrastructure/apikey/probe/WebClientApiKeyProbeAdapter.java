package me.go_gradually.omypic.infrastructure.apikey.probe;

import me.go_gradually.omypic.application.apikey.port.ApiKeyProbePort;
import me.go_gradually.omypic.infrastructure.shared.config.AppProperties;
import org.springframework.ai.openai.api.OpenAiApi;
import org.springframework.stereotype.Component;

import java.util.List;
import java.util.Locale;

@Component
public class WebClientApiKeyProbeAdapter implements ApiKeyProbePort {
    private static final String OPENAI_PROVIDER = "openai";
    private final String baseUrl;

    public WebClientApiKeyProbeAdapter(AppProperties properties) {
        this.baseUrl = properties.getIntegrations().getOpenai().getBaseUrl();
    }

    @Override
    public void probe(String provider, String apiKey, String model) throws Exception {
        validateProvider(provider);
        try {
            openAiApi(apiKey).chatCompletionEntity(new OpenAiApi.ChatCompletionRequest(
                    List.of(new OpenAiApi.ChatCompletionMessage("ping", OpenAiApi.ChatCompletionMessage.Role.USER)),
                    resolveModel(model),
                    0.0
            ));
        } catch (Exception ex) {
            throw verificationFailed(ex);
        }
    }

    private OpenAiApi openAiApi(String apiKey) {
        return OpenAiApi.builder()
                .baseUrl(baseUrl)
                .apiKey(apiKey)
                .build();
    }

    private void validateProvider(String provider) {
        if (provider == null || !OPENAI_PROVIDER.equalsIgnoreCase(provider.trim())) {
            throw new IllegalArgumentException("Unsupported provider: only openai is allowed");
        }
    }

    private String resolveModel(String model) {
        String candidate = model == null ? "" : model.trim();
        if (candidate.isBlank()) {
            return "gpt-4o-mini";
        }
        String lowered = candidate.toLowerCase(Locale.ROOT);
        if (lowered.contains("transcribe")) {
            return "gpt-4o-mini";
        }
        return candidate;
    }

    private IllegalStateException verificationFailed(Exception exception) {
        String message = exception == null || exception.getMessage() == null || exception.getMessage().isBlank()
                ? "unknown"
                : exception.getMessage();
        return new IllegalStateException("Verification failed: " + message);
    }
}
