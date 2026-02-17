package me.go_gradually.omypic.infrastructure.feedback.llm;

import java.util.Map;

final class LegacyChatModelParameterPolicy implements OpenAiModelParameterPolicy {
    @Override
    public boolean supports(String model) {
        return true;
    }

    @Override
    public void apply(Map<String, Object> payload) {
        payload.put("temperature", 0.2);
    }
}
