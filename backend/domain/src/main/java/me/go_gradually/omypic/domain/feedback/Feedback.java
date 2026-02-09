package me.go_gradually.omypic.domain.feedback;

import me.go_gradually.omypic.domain.rulebook.RulebookContext;
import me.go_gradually.omypic.domain.shared.util.TextUtils;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.stream.Collectors;

public final class Feedback {
    private final String summary;
    private final List<String> correctionPoints;
    private final String exampleAnswer;
    private final List<String> rulebookEvidence;

    private Feedback(String summary,
                     List<String> correctionPoints,
                     String exampleAnswer,
                     List<String> rulebookEvidence) {
        this.summary = summary == null ? "" : summary;
        this.correctionPoints = List.copyOf(correctionPoints == null ? List.of() : correctionPoints);
        this.exampleAnswer = exampleAnswer == null ? "" : exampleAnswer;
        this.rulebookEvidence = List.copyOf(rulebookEvidence == null ? List.of() : rulebookEvidence);
    }

    public static Feedback of(String summary,
                              List<String> correctionPoints,
                              String exampleAnswer,
                              List<String> rulebookEvidence) {
        return new Feedback(summary, correctionPoints, exampleAnswer, rulebookEvidence);
    }

    public Feedback normalized(FeedbackConstraints constraints,
                               String userText,
                               FeedbackLanguage language,
                               List<RulebookContext> contexts) {
        String normalizedSummary = TextUtils.trimToLength(summary, constraints.summaryMaxChars());

        List<String> points = new ArrayList<>(correctionPoints);
        while (points.size() < 3) {
            points.add(defaultPoint(language, points.size()));
        }
        if (points.size() > 3) {
            points = points.subList(0, 3);
        }

        String example = exampleAnswer;
        int userLength = userText == null ? 0 : userText.length();
        double ratio = (double) example.length() / Math.max(1, userLength);
        if (ratio > constraints.exampleMaxRatio()) {
            int maxLen = (int) (userLength * constraints.exampleMaxRatio());
            example = TextUtils.trimToLength(example, maxLen);
        } else if (ratio < constraints.exampleMinRatio()) {
            boolean english = language != null && language.isEnglish();
            String filler = english
                    ? " Also, consider adding one more detail."
                    : " 또한 하나의 디테일을 더 추가해 보세요.";
            example = example + filler;
        }

        List<String> evidence = rulebookEvidence;
        if (contexts == null || contexts.isEmpty()) {
            evidence = List.of();
        } else if (rulebookEvidence.isEmpty()) {
            evidence = contexts.stream()
                    .limit(1)
                    .map(ctx -> "[" + ctx.filename() + "] " + TextUtils.trimToLength(ctx.text(), 200))
                    .collect(Collectors.toList());
        }

        return Feedback.of(normalizedSummary, points, example, evidence);
    }

    private String defaultPoint(FeedbackLanguage language, int index) {
        boolean english = language != null && language.isEnglish();
        if (english) {
            return switch (index) {
                case 0 -> "Grammar: Check verb tense consistency.";
                case 1 -> "Expression: Use more specific vocabulary.";
                default -> "Logic: Add a clear reason or example.";
            };
        }
        return switch (index) {
            case 0 -> "Grammar: 시제 일관성을 점검하세요.";
            case 1 -> "Expression: 더 구체적인 어휘를 사용하세요.";
            default -> "Logic: 이유나 예시를 추가하세요.";
        };
    }

    public String getSummary() {
        return summary;
    }

    public List<String> getCorrectionPoints() {
        return Collections.unmodifiableList(correctionPoints);
    }

    public String getExampleAnswer() {
        return exampleAnswer;
    }

    public List<String> getRulebookEvidence() {
        return Collections.unmodifiableList(rulebookEvidence);
    }
}
