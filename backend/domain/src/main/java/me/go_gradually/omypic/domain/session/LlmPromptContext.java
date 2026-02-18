package me.go_gradually.omypic.domain.session;

import java.util.List;

public record LlmPromptContext(String summary,
                               List<TurnRecord> recentTurns,
                               List<RecommendationRecord> recentRecommendations) {
    public LlmPromptContext {
        summary = normalize(summary);
        recentTurns = safeTurns(recentTurns);
        recentRecommendations = safeRecommendations(recentRecommendations);
    }

    public static LlmPromptContext empty() {
        return new LlmPromptContext("", List.of(), List.of());
    }

    public record TurnRecord(String question, String answer, String feedbackSummary) {
        public TurnRecord {
            question = normalize(question);
            answer = normalize(answer);
            feedbackSummary = normalize(feedbackSummary);
        }
    }

    public record RecommendationRecord(String fillerTerm, String adjectiveTerm, String adverbTerm) {
        public RecommendationRecord {
            fillerTerm = normalize(fillerTerm);
            adjectiveTerm = normalize(adjectiveTerm);
            adverbTerm = normalize(adverbTerm);
        }
    }

    private static List<TurnRecord> safeTurns(List<TurnRecord> turns) {
        if (turns == null || turns.isEmpty()) {
            return List.of();
        }
        return turns.stream()
                .filter(turn -> turn != null)
                .toList();
    }

    private static List<RecommendationRecord> safeRecommendations(List<RecommendationRecord> records) {
        if (records == null || records.isEmpty()) {
            return List.of();
        }
        return records.stream()
                .filter(record -> record != null)
                .toList();
    }

    private static String normalize(String value) {
        return value == null ? "" : value.trim();
    }
}
