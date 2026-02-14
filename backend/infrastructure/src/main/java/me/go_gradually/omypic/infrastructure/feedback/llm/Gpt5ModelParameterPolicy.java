package me.go_gradually.omypic.infrastructure.feedback.llm;

import java.util.Locale;
import java.util.Map;
import java.util.Set;

final class Gpt5ModelParameterPolicy implements OpenAiModelParameterPolicy {
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
        String lowered = model == null ? "" : model.toLowerCase(Locale.ROOT);
        return lowered.startsWith("gpt-5");
    }

    @Override
    public void apply(Map<String, Object> payload) {
        // GPT-5 family ignores temperature in this client path.
    }

    @Override
    public Set<String> retryRemovableParameters() {
        return RETRY_REMOVABLE;
    }
}
