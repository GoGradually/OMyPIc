package me.go_gradually.omypic.domain.session;

import java.util.List;

public record LlmPromptContext(String summary, List<TurnRecord> recentTurns) {
    public LlmPromptContext {
        summary = normalize(summary);
        recentTurns = recentTurns == null ? List.of() : List.copyOf(recentTurns);
    }

    public static LlmPromptContext empty() {
        return new LlmPromptContext("", List.of());
    }

    public record TurnRecord(String question, String answer, String feedbackSummary) {
        public TurnRecord {
            question = normalize(question);
            answer = normalize(answer);
            feedbackSummary = normalize(feedbackSummary);
        }
    }

    private static String normalize(String value) {
        return value == null ? "" : value.trim();
    }
}
