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
    private static final String OPENAI_PROVIDER = "openai";

    public WebClientApiKeyProbeAdapter(@Qualifier("openAiWebClient") WebClient openAiWebClient) {
        this.openAiWebClient = openAiWebClient;
    }

    @Override
    public void probe(String provider, String apiKey, String model) throws Exception {
        validateProvider(provider);
        try {
            probeOpenAi(apiKey);
        } catch (WebClientResponseException ex) {
            throw verificationFailed(ex.getStatusCode());
        }
    }

    private void validateProvider(String provider) {
        if (provider == null || !OPENAI_PROVIDER.equalsIgnoreCase(provider.trim())) {
            throw new IllegalArgumentException("Unsupported provider: only openai is allowed");
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

    private IllegalStateException verificationFailed(HttpStatusCode status) {
        return new IllegalStateException("Verification failed with status " + status.value());
    }
}
