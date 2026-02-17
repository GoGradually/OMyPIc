package me.go_gradually.omypic.infrastructure.feedback.llm;

import java.util.Map;

interface OpenAiModelParameterPolicy {
    boolean supports(String model);

    void apply(Map<String, Object> payload);
}
