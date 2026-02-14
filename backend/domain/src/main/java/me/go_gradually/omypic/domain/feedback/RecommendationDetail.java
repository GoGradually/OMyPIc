package me.go_gradually.omypic.domain.feedback;

public record RecommendationDetail(String term, String usage) {
    public RecommendationDetail {
        term = term == null ? "" : term.trim();
        usage = usage == null ? "" : usage.trim();
    }
}
