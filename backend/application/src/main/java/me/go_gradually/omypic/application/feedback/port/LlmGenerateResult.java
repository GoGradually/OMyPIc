package me.go_gradually.omypic.application.feedback.port;

import me.go_gradually.omypic.domain.feedback.Feedback;

import java.util.List;

public record LlmGenerateResult(Feedback feedback, List<String> schemaFallbackReasons) {
    public LlmGenerateResult {
        feedback = feedback == null ? Feedback.of("", List.of(), List.of(), "", List.of()) : feedback;
        schemaFallbackReasons = schemaFallbackReasons == null ? List.of() : List.copyOf(schemaFallbackReasons);
    }
}
