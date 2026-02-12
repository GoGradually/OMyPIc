package me.go_gradually.omypic.domain.feedback;

import me.go_gradually.omypic.domain.rulebook.RulebookContext;
import me.go_gradually.omypic.domain.shared.util.TextUtils;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Locale;
import java.util.Set;
import java.util.stream.Collectors;

public final class Feedback {
    private static final List<String> CATEGORY_ORDER = List.of("Grammar", "Expression", "Logic");
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
        String normalizedSummary = normalizeSummary(language, constraints.summaryMaxChars());
        List<String> points = normalizeCorrectionPoints(language);
        String example = normalizeExampleAnswer(userText, language, constraints.exampleMinRatio(), constraints.exampleMaxRatio());
        List<String> evidence = normalizeRulebookEvidence(contexts);
        return Feedback.of(normalizedSummary, points, example, evidence);
    }

    private String normalizeSummary(FeedbackLanguage language, int summaryMaxChars) {
        String base = summaryBase(language);
        List<String> sentences = splitSentences(base);
        String selected = selectSummary(sentences, language);
        return trimSummary(selected, language, summaryMaxChars);
    }

    private String summaryBase(FeedbackLanguage language) {
        String base = summary == null ? "" : summary.trim();
        return base.isEmpty() ? defaultSummary(language) : base;
    }

    private List<String> splitSentences(String text) {
        return Arrays.stream(text.split("(?<=[.!?])\\s+|\\n+"))
                .map(String::trim)
                .filter(s -> !s.isEmpty())
                .toList();
    }

    private String selectSummary(List<String> sentences, FeedbackLanguage language) {
        if (sentences.isEmpty()) {
            return defaultSummary(language);
        }
        if (sentences.size() == 1) {
            return sentences.get(0);
        }
        return sentences.get(0) + " " + sentences.get(1);
    }

    private String trimSummary(String selected, FeedbackLanguage language, int maxChars) {
        String trimmed = TextUtils.trimToLength(selected, maxChars).trim();
        if (!trimmed.isEmpty()) {
            return trimmed;
        }
        return TextUtils.trimToLength(defaultSummary(language), maxChars).trim();
    }

    private List<String> normalizeCorrectionPoints(FeedbackLanguage language) {
        List<String> points = correctionPoints.stream()
                .filter(p -> p != null && !p.isBlank())
                .map(String::trim)
                .collect(Collectors.toCollection(ArrayList::new));
        trimToMax(points, 3);
        appendMissingCategories(points, language);
        fillUntilThree(points, language);
        trimToMax(points, 3);
        ensureCategoryCoverageByReplacement(points, language);
        return points;
    }

    private void fillUntilThree(List<String> points, FeedbackLanguage language) {
        while (points.size() < 3) {
            points.add(defaultPoint(language, points.size()));
        }
    }

    private void appendMissingCategories(List<String> points, FeedbackLanguage language) {
        Set<String> categories = categoriesOf(points);
        for (String category : CATEGORY_ORDER) {
            if (categories.size() >= 2) {
                return;
            }
            if (categories.contains(category)) {
                continue;
            }
            points.add(defaultPointForCategory(language, category));
            categories.add(category);
        }
    }

    private void ensureCategoryCoverageByReplacement(List<String> points, FeedbackLanguage language) {
        if (points.isEmpty() || categoriesOf(points).size() >= 2) {
            return;
        }
        replaceLastPointWithMissingCategory(points, language);
    }

    private void replaceLastPointWithMissingCategory(List<String> points, FeedbackLanguage language) {
        for (String category : CATEGORY_ORDER) {
            if (categoriesOf(points).contains(category)) {
                continue;
            }
            points.set(points.size() - 1, defaultPointForCategory(language, category));
            if (categoriesOf(points).size() >= 2) {
                return;
            }
        }
    }

    private void trimToMax(List<String> points, int maxSize) {
        if (points.size() <= maxSize) {
            return;
        }
        points.subList(maxSize, points.size()).clear();
    }

    private Set<String> categoriesOf(List<String> points) {
        Set<String> categories = new LinkedHashSet<>();
        for (String point : points) {
            String category = detectCategory(point);
            if (category != null) {
                categories.add(category);
            }
        }
        return categories;
    }

    private String detectCategory(String point) {
        if (point == null) {
            return null;
        }
        String lowered = point.toLowerCase(Locale.ROOT);
        if (lowered.contains("grammar") || lowered.contains("문법")) {
            return "Grammar";
        }
        if (lowered.contains("expression") || lowered.contains("표현")) {
            return "Expression";
        }
        if (lowered.contains("logic") || lowered.contains("논리")) {
            return "Logic";
        }
        return null;
    }

    private String normalizeExampleAnswer(String userText,
                                          FeedbackLanguage language,
                                          double minRatio,
                                          double maxRatio) {
        String normalized = exampleAnswer == null ? "" : exampleAnswer.trim();
        int userLength = userText == null ? 0 : userText.trim().length();
        if (userLength <= 0) {
            if (!normalized.isEmpty()) {
                return normalized;
            }
            return defaultExampleSeed(language, userText);
        }

        int minLength = Math.max(1, (int) Math.ceil(userLength * minRatio));
        int maxLength = Math.max(minLength, (int) Math.floor(userLength * maxRatio));
        if (normalized.isEmpty()) {
            normalized = defaultExampleSeed(language, userText);
        }
        if (normalized.length() > maxLength) {
            normalized = normalized.substring(0, maxLength).trim();
        }
        String filler = language != null && language.isEnglish()
                ? " Add one concrete detail."
                : " 구체적인 디테일을 한 문장 더 추가하세요.";
        while (normalized.length() < minLength) {
            normalized = (normalized + filler).trim();
            if (normalized.length() > maxLength) {
                normalized = normalized.substring(0, maxLength).trim();
                break;
            }
        }
        if (normalized.length() < minLength) {
            String fallback = userText == null ? "" : userText.trim();
            if (!fallback.isEmpty()) {
                normalized = fallback;
            }
            if (normalized.length() > maxLength) {
                normalized = normalized.substring(0, maxLength).trim();
            }
            while (normalized.length() < minLength) {
                normalized = normalized + (language != null && language.isEnglish() ? " detail" : " 디테일");
                if (normalized.length() > maxLength) {
                    normalized = normalized.substring(0, maxLength).trim();
                    break;
                }
            }
        }
        return normalized;
    }

    private List<String> normalizeRulebookEvidence(List<RulebookContext> contexts) {
        if (contexts == null || contexts.isEmpty()) {
            return List.of();
        }
        List<String> cleaned = cleanedEvidence();
        return cleaned.isEmpty() ? fallbackEvidence(contexts) : cleaned;
    }

    private List<String> cleanedEvidence() {
        return rulebookEvidence.stream()
                .filter(e -> e != null && !e.isBlank())
                .map(String::trim)
                .toList();
    }

    private List<String> fallbackEvidence(List<RulebookContext> contexts) {
        return contexts.stream()
                .limit(1)
                .map(ctx -> "[" + ctx.filename() + "] " + TextUtils.trimToLength(ctx.text(), 200))
                .collect(Collectors.toList());
    }

    private String defaultSummary(FeedbackLanguage language) {
        if (language != null && language.isEnglish()) {
            return "Your answer is understandable and can be improved with clearer structure.";
        }
        return "답변의 핵심은 전달되었습니다. 구조를 더 선명하게 다듬으면 더 좋아집니다.";
    }

    private String defaultExampleSeed(FeedbackLanguage language, String userText) {
        String base = userText == null ? "" : userText.trim();
        if (!base.isEmpty()) {
            return base;
        }
        if (language != null && language.isEnglish()) {
            return "I can explain my idea clearly with one concrete example.";
        }
        return "저는 핵심 의견을 한 가지 예시와 함께 분명하게 설명할 수 있습니다.";
    }

    private String defaultPointForCategory(FeedbackLanguage language, String category) {
        return switch (category) {
            case "Grammar" -> language != null && language.isEnglish()
                    ? "Grammar: Check verb tense consistency."
                    : "Grammar: 시제 일관성을 점검하세요.";
            case "Expression" -> language != null && language.isEnglish()
                    ? "Expression: Use more specific vocabulary."
                    : "Expression: 더 구체적인 어휘를 사용하세요.";
            default -> language != null && language.isEnglish()
                    ? "Logic: Add a clear reason or example."
                    : "Logic: 이유나 예시를 추가하세요.";
        };
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
