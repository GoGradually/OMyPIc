package me.go_gradually.omypic.infrastructure.feedback.llm;

import java.util.Map;
import java.util.Set;

interface OpenAiModelParameterPolicy {
    boolean supports(String model);

    void apply(Map<String, Object> payload);

    Set<String> retryRemovableParameters();
}
