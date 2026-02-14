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

        WebClient openAiClient = config.openAiWebClient(properties);

        assertNotNull(openAiClient);
    }
}
