package me.go_gradually.omypic.infrastructure.feedback.llm;

import java.util.Map;
import java.util.Set;

final class LegacyChatModelParameterPolicy implements OpenAiModelParameterPolicy {
    private static final Set<String> RETRY_REMOVABLE = Set.of(
            "temperature",
            "response_format",
            "reasoning",
            "top_p",
            "max_tokens",
            "max_completion_tokens"
    );

    @Override
    public boolean supports(String model) {
        return true;
    }

    @Override
    public void apply(Map<String, Object> payload) {
        payload.put("temperature", 0.2);
    }

    @Override
    public Set<String> retryRemovableParameters() {
        return RETRY_REMOVABLE;
    }
}
