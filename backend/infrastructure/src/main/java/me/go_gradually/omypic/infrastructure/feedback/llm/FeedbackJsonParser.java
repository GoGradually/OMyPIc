package me.go_gradually.omypic.infrastructure.feedback.llm;

import com.fasterxml.jackson.databind.JsonNode;
import me.go_gradually.omypic.domain.feedback.CorrectionDetail;
import me.go_gradually.omypic.domain.feedback.Corrections;
import me.go_gradually.omypic.domain.feedback.Feedback;
import me.go_gradually.omypic.domain.feedback.RecommendationDetail;
import me.go_gradually.omypic.domain.feedback.Recommendations;

import java.util.ArrayList;
import java.util.List;

/**
 * Maps the feedback JSON document an LLM returns onto the domain model.
 * Accepts both the structured shape (corrections/recommendations objects)
 * and the older flat shape (correctionPoints/recommendation arrays).
 */
final class FeedbackJsonParser {
    private FeedbackJsonParser() {
    }

    static Feedback parse(JsonNode root) {
        String summary = root.path("summary").asText("");
        String exampleAnswer = root.path("exampleAnswer").asText("");
        List<String> evidence = toStringList(root.path("rulebookEvidence"));
        JsonNode corrections = root.path("corrections");
        JsonNode recommendations = root.path("recommendations");

        if (!corrections.isObject() && !recommendations.isObject()) {
            return parseFlat(root, summary, exampleAnswer, evidence);
        }
        return Feedback.of(
                summary, parseCorrections(corrections), parseRecommendations(recommendations), exampleAnswer, evidence);
    }

    private static Feedback parseFlat(JsonNode root, String summary, String exampleAnswer, List<String> evidence) {
        List<String> correctionPoints = toStringList(root.path("correctionPoints"));
        List<String> recommendation = toStringList(root.path("recommendation"));
        return Feedback.of(summary, correctionPoints, recommendation, exampleAnswer, evidence);
    }

    /**
     * Cuts the outermost JSON object out of a response that may carry text around it.
     */
    static String extractJson(String raw) {
        if (raw == null || raw.isBlank()) {
            return "{}";
        }
        int start = raw.indexOf('{');
        int end = raw.lastIndexOf('}');
        if (start >= 0 && end > start) {
            return raw.substring(start, end + 1);
        }
        return raw;
    }

    private static Corrections parseCorrections(JsonNode node) {
        return new Corrections(
                parseCorrectionDetail(node.path("grammar")),
                parseCorrectionDetail(node.path("expression")),
                parseCorrectionDetail(node.path("logic"))
        );
    }

    private static Recommendations parseRecommendations(JsonNode node) {
        return new Recommendations(
                parseRecommendationDetail(node.path("filler")),
                parseRecommendationDetail(node.path("adjective")),
                parseRecommendationDetail(node.path("adverb"))
        );
    }

    private static CorrectionDetail parseCorrectionDetail(JsonNode node) {
        if (node == null || !node.isObject()) {
            return new CorrectionDetail("", "");
        }
        return new CorrectionDetail(node.path("issue").asText(""), node.path("fix").asText(""));
    }

    private static RecommendationDetail parseRecommendationDetail(JsonNode node) {
        if (node == null || !node.isObject()) {
            return new RecommendationDetail("", "");
        }
        return new RecommendationDetail(node.path("term").asText(""), node.path("usage").asText(""));
    }

    private static List<String> toStringList(JsonNode arrayNode) {
        List<String> values = new ArrayList<>();
        if (arrayNode == null || !arrayNode.isArray()) {
            return values;
        }
        for (JsonNode node : arrayNode) {
            values.add(node.asText(""));
        }
        return values;
    }
}
