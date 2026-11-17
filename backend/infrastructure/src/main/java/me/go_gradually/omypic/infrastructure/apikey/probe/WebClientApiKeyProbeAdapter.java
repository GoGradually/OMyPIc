package me.go_gradually.omypic.infrastructure.apikey.probe;

import com.anthropic.errors.AnthropicIoException;
import com.anthropic.errors.AnthropicServiceException;
import com.anthropic.errors.NotFoundException;
import com.anthropic.errors.UnauthorizedException;
import me.go_gradually.omypic.application.apikey.port.ApiKeyProbePort;
import me.go_gradually.omypic.application.feedback.policy.FeedbackModelPolicy;
import me.go_gradually.omypic.infrastructure.shared.anthropic.AnthropicClientFactory;
import me.go_gradually.omypic.infrastructure.shared.config.AppProperties;
import org.springframework.ai.openai.api.OpenAiApi;
import org.springframework.stereotype.Component;

import java.util.List;
import java.util.Locale;

@Component
public class WebClientApiKeyProbeAdapter implements ApiKeyProbePort {
    private final String baseUrl;
    private final AnthropicClientFactory anthropicClientFactory;

    public WebClientApiKeyProbeAdapter(AppProperties properties, AnthropicClientFactory anthropicClientFactory) {
        this.baseUrl = properties.getIntegrations().getOpenai().getBaseUrl();
        this.anthropicClientFactory = anthropicClientFactory;
    }

    @Override
    public void probe(String provider, String apiKey, String model) throws Exception {
        String normalized = FeedbackModelPolicy.normalizeProvider(provider);
        if (FeedbackModelPolicy.ANTHROPIC_PROVIDER.equals(normalized)) {
            probeAnthropic(apiKey, model);
            return;
        }
        probeOpenAi(apiKey, model);
    }

    private void probeOpenAi(String apiKey, String model) {
        try {
            openAiApi(apiKey).chatCompletionEntity(new OpenAiApi.ChatCompletionRequest(
                    List.of(new OpenAiApi.ChatCompletionMessage("ping", OpenAiApi.ChatCompletionMessage.Role.USER)),
                    resolveOpenAiModel(model),
                    0.0
            ));
        } catch (Exception ex) {
            throw verificationFailed(ex.getMessage());
        }
    }

    /**
     * Looks the model up instead of sending a message: it checks the key and the model id
     * in one call without spending tokens.
     */
    private void probeAnthropic(String apiKey, String model) {
        String resolvedModel = resolveAnthropicModel(model);
        try {
            anthropicClientFactory.forApiKey(apiKey).models().retrieve(resolvedModel);
        } catch (AnthropicServiceException e) {
            throw verificationFailed(describe(e, resolvedModel));
        } catch (AnthropicIoException e) {
            throw verificationFailed("could not reach the Anthropic API");
        }
    }

    private String describe(AnthropicServiceException error, String model) {
        if (error instanceof UnauthorizedException) {
            return "the Anthropic API key was rejected";
        }
        if (error instanceof NotFoundException) {
            return "this key has no access to " + model;
        }
        return "HTTP " + error.statusCode();
    }

    private OpenAiApi openAiApi(String apiKey) {
        return OpenAiApi.builder()
                .baseUrl(baseUrl)
                .apiKey(apiKey)
                .build();
    }

    private String resolveOpenAiModel(String model) {
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

    private String resolveAnthropicModel(String model) {
        if (model == null || model.isBlank()) {
            return FeedbackModelPolicy.defaultModel(FeedbackModelPolicy.ANTHROPIC_PROVIDER);
        }
        return model.trim().toLowerCase(Locale.ROOT);
    }

    private IllegalStateException verificationFailed(String reason) {
        String message = reason == null || reason.isBlank() ? "unknown" : reason;
        return new IllegalStateException("Verification failed: " + message);
    }
}
