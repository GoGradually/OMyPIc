package me.go_gradually.omypic.infrastructure.feedback.llm;

import me.go_gradually.omypic.domain.session.LlmPromptContext;

import java.util.List;

/**
 * Renders the session memory (summary, recent turns, recent recommendation terms)
 * in front of the current turn prompt. Shared by every LLM provider.
 */
final class LlmPromptContextRenderer {
    private LlmPromptContextRenderer() {
    }

    static String merge(String userPrompt, LlmPromptContext promptContext) {
        String prompt = safe(userPrompt);
        if (isEmpty(promptContext)) {
            return prompt;
        }
        StringBuilder builder = new StringBuilder("# Previous context\n");
        appendSummary(builder, safe(promptContext.summary()));
        appendTurns(builder, promptContext.recentTurns());
        appendRecommendations(builder, promptContext.recentRecommendations());
        return builder.append("# Current input\n").append(prompt).toString();
    }

    private static boolean isEmpty(LlmPromptContext context) {
        if (context == null) {
            return true;
        }
        return safe(context.summary()).isBlank() && isEmpty(context.recentTurns())
                && isEmpty(context.recentRecommendations());
    }

    private static boolean isEmpty(List<?> values) {
        return values == null || values.isEmpty();
    }

    private static void appendSummary(StringBuilder builder, String summary) {
        if (!summary.isBlank()) {
            builder.append("Summary:\n").append(summary).append("\n\n");
        }
    }

    private static void appendTurns(StringBuilder builder, List<LlmPromptContext.TurnRecord> turns) {
        if (isEmpty(turns)) {
            return;
        }
        builder.append("Recent turns:\n");
        int index = 1;
        for (LlmPromptContext.TurnRecord turn : turns) {
            builder.append(index++)
                    .append(") Q: ").append(safe(turn.question())).append("\n")
                    .append("A: ").append(safe(turn.answer())).append("\n")
                    .append("Feedback summary: ").append(safe(turn.feedbackSummary())).append("\n");
        }
        builder.append("\n");
    }

    private static void appendRecommendations(StringBuilder builder,
                                              List<LlmPromptContext.RecommendationRecord> records) {
        if (isEmpty(records)) {
            return;
        }
        builder.append("Recent recommendation terms:\n");
        int index = 1;
        for (LlmPromptContext.RecommendationRecord record : records) {
            builder.append(index++)
                    .append(") Filler: ").append(safe(record.fillerTerm()))
                    .append(", Adjective: ").append(safe(record.adjectiveTerm()))
                    .append(", Adverb: ").append(safe(record.adverbTerm()))
                    .append("\n");
        }
        builder.append("\n");
    }

    private static String safe(String value) {
        return value == null ? "" : value.trim();
    }
}
