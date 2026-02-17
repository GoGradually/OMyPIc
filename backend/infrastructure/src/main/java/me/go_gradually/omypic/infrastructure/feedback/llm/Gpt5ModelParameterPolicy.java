package me.go_gradually.omypic.infrastructure.feedback.llm;

import java.util.Locale;
import java.util.Map;

final class Gpt5ModelParameterPolicy implements OpenAiModelParameterPolicy {
    @Override
    public boolean supports(String model) {
        String lowered = model == null ? "" : model.toLowerCase(Locale.ROOT);
        return lowered.startsWith("gpt-5");
    }

    @Override
    public void apply(Map<String, Object> payload) {
        // GPT-5 family ignores temperature in this client path.
    }
}
