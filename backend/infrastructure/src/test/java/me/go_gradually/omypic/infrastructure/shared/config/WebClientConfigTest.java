package me.go_gradually.omypic.infrastructure.shared.config;

import org.junit.jupiter.api.Test;
import org.springframework.web.reactive.function.client.WebClient;

import static org.junit.jupiter.api.Assertions.assertNotNull;

class WebClientConfigTest {

    @Test
    void webClient_buildsConfiguredClient() {
        WebClientConfig config = new WebClientConfig();

        WebClient client = config.webClient();

        assertNotNull(client);
    }
}
