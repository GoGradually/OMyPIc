package me.go_gradually.omypic.application.feedback.port;

import me.go_gradually.omypic.domain.feedback.Feedback;
import me.go_gradually.omypic.domain.session.LlmConversationState;

import java.util.List;

public record LlmGenerateResult(Feedback feedback,
                                List<String> schemaFallbackReasons,
                                LlmConversationState conversationState,
                                String summaryForNextTurn) {
    public LlmGenerateResult {
        feedback = feedback == null ? Feedback.of("", List.of(), List.of(), "", List.of()) : feedback;
        schemaFallbackReasons = schemaFallbackReasons == null ? List.of() : List.copyOf(schemaFallbackReasons);
        conversationState = conversationState == null ? LlmConversationState.empty() : conversationState;
        summaryForNextTurn = summaryForNextTurn == null ? "" : summaryForNextTurn.trim();
    }
}
