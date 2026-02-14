package me.go_gradually.omypic.domain.feedback;

public record Recommendations(RecommendationDetail filler,
                              RecommendationDetail adjective,
                              RecommendationDetail adverb) {
    public Recommendations {
        filler = filler == null ? new RecommendationDetail("", "") : filler;
        adjective = adjective == null ? new RecommendationDetail("", "") : adjective;
        adverb = adverb == null ? new RecommendationDetail("", "") : adverb;
    }
}
