package me.go_gradually.omypic.infrastructure.shared.anthropic;

import com.anthropic.client.AnthropicClient;
import com.anthropic.client.okhttp.AnthropicOkHttpClient;
import jakarta.annotation.PreDestroy;
import me.go_gradually.omypic.infrastructure.shared.config.AppProperties;
import org.springframework.stereotype.Component;

import java.time.Duration;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;

/**
 * Hands out Anthropic SDK clients for user-supplied API keys.
 * The key arrives per request (it is stored in the OS keychain, never on the backend),
 * so clients are cached by key to reuse their connection pool across feedback turns.
 */
@Component
public class AnthropicClientFactory {
    private static final int MAX_CACHED_CLIENTS = 4;
    private final Map<String, AnthropicClient> clientsByApiKey = new ConcurrentHashMap<>();
    private final String baseUrl;
    private final Duration timeout;
    private final int maxRetries;

    public AnthropicClientFactory(AppProperties properties) {
        AppProperties.Anthropic anthropic = properties.getIntegrations().getAnthropic();
        this.baseUrl = stripTrailingSlash(anthropic.getBaseUrl());
        this.timeout = Duration.ofSeconds(Math.max(1, anthropic.getTimeoutSeconds()));
        this.maxRetries = Math.max(0, anthropic.getMaxRetries());
    }

    public AnthropicClient forApiKey(String apiKey) {
        if (apiKey == null || apiKey.isBlank()) {
            throw new IllegalArgumentException("Anthropic API key is required");
        }
        AnthropicClient cached = clientsByApiKey.get(apiKey);
        if (cached != null) {
            return cached;
        }
        evictIfFull();
        return clientsByApiKey.computeIfAbsent(apiKey, this::build);
    }

    @PreDestroy
    public void close() {
        clientsByApiKey.values().forEach(AnthropicClient::close);
        clientsByApiKey.clear();
    }

    private String stripTrailingSlash(String url) {
        String value = url == null ? "" : url.trim();
        return value.endsWith("/") ? value.substring(0, value.length() - 1) : value;
    }

    private void evictIfFull() {
        if (clientsByApiKey.size() >= MAX_CACHED_CLIENTS) {
            // Dropped clients are not closed here: a feedback turn may still be using one.
            // Their idle connections and threads are reclaimed by OkHttp on its own.
            clientsByApiKey.clear();
        }
    }

    private AnthropicClient build(String apiKey) {
        return AnthropicOkHttpClient.builder()
                .apiKey(apiKey)
                .baseUrl(baseUrl)
                .timeout(timeout)
                .maxRetries(maxRetries)
                .build();
    }
}
