package me.go_gradually.omypic.infrastructure.feedback.llm;

import org.junit.jupiter.api.Test;

import java.util.HashMap;
import java.util.Map;

import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

class OpenAiModelParameterPolicyResolverTest {

    private final OpenAiModelParameterPolicyResolver resolver = new OpenAiModelParameterPolicyResolver();

    @Test
    void resolve_returnsGpt5PolicyForGpt5Family() {
        OpenAiModelParameterPolicy policy = resolver.resolve("gpt-5-mini");
        Map<String, Object> payload = new HashMap<>();

        policy.apply(payload);

        assertFalse(payload.containsKey("temperature"));
    }

    @Test
    void resolve_returnsLegacyPolicyForNonGpt5Model() {
        OpenAiModelParameterPolicy policy = resolver.resolve("gpt-4o-mini");
        Map<String, Object> payload = new HashMap<>();

        policy.apply(payload);

        assertTrue(payload.containsKey("temperature"));
    }

    @Test
    void retryRemovableParameters_containsTemperature() {
        assertTrue(resolver.isRetryRemovableParameter("temperature"));
    }
}
