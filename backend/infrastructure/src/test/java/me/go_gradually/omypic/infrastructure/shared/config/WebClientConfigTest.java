package me.go_gradually.omypic.infrastructure.shared.config;

import org.junit.jupiter.api.Test;
import org.springframework.web.reactive.function.client.WebClient;

import static org.junit.jupiter.api.Assertions.assertNotNull;

class WebClientConfigTest {

    @Test
    void webClients_buildConfiguredClients() {
        WebClientConfig config = new WebClientConfig();
        AppProperties properties = new AppProperties();
        properties.getIntegrations().getOpenai().setBaseUrl("http://openai.local");
        properties.getIntegrations().getGemini().setBaseUrl("http://gemini.local");
        properties.getIntegrations().getAnthropic().setBaseUrl("http://anthropic.local");

        WebClient openAiClient = config.openAiWebClient(properties);
        WebClient geminiClient = config.geminiWebClient(properties);
        WebClient anthropicClient = config.anthropicWebClient(properties);

        assertNotNull(openAiClient);
        assertNotNull(geminiClient);
        assertNotNull(anthropicClient);
    }
}
