package me.go_gradually.omypic.infrastructure.apikey.probe;

import me.go_gradually.omypic.application.apikey.port.ApiKeyProbePort;
import org.springframework.beans.factory.annotation.Qualifier;
import org.springframework.http.HttpStatusCode;
import org.springframework.stereotype.Component;
import org.springframework.web.reactive.function.client.WebClient;
import org.springframework.web.reactive.function.client.WebClientResponseException;

@Component
public class WebClientApiKeyProbeAdapter implements ApiKeyProbePort {
    private final WebClient openAiWebClient;
    private final WebClient geminiWebClient;
    private final WebClient anthropicWebClient;

    public WebClientApiKeyProbeAdapter(@Qualifier("openAiWebClient") WebClient openAiWebClient,
                                       @Qualifier("geminiWebClient") WebClient geminiWebClient,
                                       @Qualifier("anthropicWebClient") WebClient anthropicWebClient) {
        this.openAiWebClient = openAiWebClient;
        this.geminiWebClient = geminiWebClient;
        this.anthropicWebClient = anthropicWebClient;
    }

    @Override
    public void probe(String provider, String apiKey, String model) throws Exception {
        try {
            probeProvider(provider, apiKey);
        } catch (WebClientResponseException ex) {
            throw verificationFailed(ex.getStatusCode());
        }
    }

    private void probeProvider(String provider, String apiKey) {
        switch (provider) {
            case "openai" -> probeOpenAi(apiKey);
            case "gemini" -> probeGemini(apiKey);
            case "anthropic" -> probeAnthropic(apiKey);
            default -> throw new IllegalArgumentException("Unsupported provider");
        }
    }

    private void probeOpenAi(String apiKey) {
        openAiWebClient.get()
                .uri("/v1/models")
                .header("Authorization", "Bearer " + apiKey)
                .retrieve()
                .toBodilessEntity()
                .block();
    }

    private void probeGemini(String apiKey) {
        geminiWebClient.get()
                .uri("/v1beta/models?key=" + apiKey)
                .retrieve()
                .toBodilessEntity()
                .block();
    }

    private void probeAnthropic(String apiKey) {
        anthropicWebClient.get()
                .uri("/v1/models")
                .header("x-api-key", apiKey)
                .header("anthropic-version", "2023-06-01")
                .retrieve()
                .toBodilessEntity()
                .block();
    }

    private IllegalStateException verificationFailed(HttpStatusCode status) {
        return new IllegalStateException("Verification failed with status " + status.value());
    }
}
