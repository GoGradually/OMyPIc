package me.go_gradually.omypic.infrastructure.shared.config;

import org.springframework.context.annotation.Bean;
import org.springframework.context.annotation.Configuration;
import org.springframework.http.client.reactive.ReactorClientHttpConnector;
import org.springframework.web.reactive.function.client.ExchangeStrategies;
import org.springframework.web.reactive.function.client.WebClient;
import reactor.netty.http.client.HttpClient;
import reactor.netty.resources.ConnectionProvider;

import java.time.Duration;

@Configuration
public class WebClientConfig {
    @Bean("openAiWebClient")
    public WebClient openAiWebClient(AppProperties properties) {
        return createWebClient(properties.getIntegrations().getOpenai().getBaseUrl());
    }

    @Bean("geminiWebClient")
    public WebClient geminiWebClient(AppProperties properties) {
        return createWebClient(properties.getIntegrations().getGemini().getBaseUrl());
    }

    @Bean("anthropicWebClient")
    public WebClient anthropicWebClient(AppProperties properties) {
        return createWebClient(properties.getIntegrations().getAnthropic().getBaseUrl());
    }

    private WebClient createWebClient(String baseUrl) {
        ConnectionProvider provider = createConnectionProvider();
        HttpClient httpClient = createHttpClient(provider);
        ExchangeStrategies strategies = createExchangeStrategies();
        return WebClient.builder()
                .baseUrl(baseUrl)
                .clientConnector(new ReactorClientHttpConnector(httpClient))
                .exchangeStrategies(strategies)
                .build();
    }

    private ConnectionProvider createConnectionProvider() {
        return ConnectionProvider.builder("omypic-http")
                .maxConnections(50)
                .pendingAcquireTimeout(Duration.ofSeconds(30))
                .build();
    }

    private HttpClient createHttpClient(ConnectionProvider provider) {
        return HttpClient.create(provider).responseTimeout(Duration.ofSeconds(60));
    }

    private ExchangeStrategies createExchangeStrategies() {
        return ExchangeStrategies.builder()
                .codecs(configurer -> configurer.defaultCodecs().maxInMemorySize(20 * 1024 * 1024))
                .build();
    }
}
