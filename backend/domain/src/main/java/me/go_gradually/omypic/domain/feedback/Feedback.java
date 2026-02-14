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
    private final Corrections corrections;
    private final Recommendations recommendations;
    private final String exampleAnswer;
    private final List<String> rulebookEvidence;

    private Feedback(String summary,
                     Corrections corrections,
                     Recommendations recommendations,
                     String exampleAnswer,
                     List<String> rulebookEvidence) {
        this.summary = summary == null ? "" : summary;
        this.corrections = corrections == null ? new Corrections(null, null, null) : corrections;
        this.recommendations = recommendations == null ? new Recommendations(null, null, null) : recommendations;
        this.exampleAnswer = exampleAnswer == null ? "" : exampleAnswer;
        this.rulebookEvidence = List.copyOf(rulebookEvidence == null ? List.of() : rulebookEvidence);
    }

    public static Feedback of(String summary,
                              Corrections corrections,
                              Recommendations recommendations,
                              String exampleAnswer,
                              List<String> rulebookEvidence) {
        return new Feedback(summary, corrections, recommendations, exampleAnswer, rulebookEvidence);
    }

    public static Feedback of(String summary,
                              List<String> correctionPoints,
                              List<String> recommendation,
                              String exampleAnswer,
                              List<String> rulebookEvidence) {
        List<String> mergedRecommendationSources = mergePoints(correctionPoints, recommendation);
        return new Feedback(
                summary,
                toCorrections(correctionPoints),
                toRecommendations(mergedRecommendationSources),
                exampleAnswer,
                rulebookEvidence
        );
    }

    public static Feedback of(String summary,
                              List<String> correctionPoints,
                              String exampleAnswer,
                              List<String> rulebookEvidence) {
        return new Feedback(summary, toCorrections(correctionPoints), toRecommendations(correctionPoints), exampleAnswer, rulebookEvidence);
    }

    public Feedback normalized(FeedbackConstraints constraints,
                               String userText,
                               FeedbackLanguage language,
                               List<RulebookContext> contexts) {
        String normalizedSummary = normalizeSummary(language, constraints.summaryMaxChars());
        List<String> points = normalizeCorrectionPoints(language);
        List<String> recommendationPoints = normalizeRecommendationPoints(language);
        String example = normalizeExampleAnswer(userText, language, constraints.exampleMinRatio(), constraints.exampleMaxRatio());
        List<String> evidence = normalizeRulebookEvidence(contexts);
        return Feedback.of(
                normalizedSummary,
                toCorrections(points),
                toRecommendations(recommendationPoints),
                example,
                evidence
        );
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
        List<String> rawPoints = collectRawPoints();
        List<String> points = rawPoints.stream()
                .filter(point -> !isAuxiliaryPoint(point))
                .collect(Collectors.toCollection(ArrayList::new));
        trimToMax(points, 3);
        appendMissingCategories(points, language);
        fillUntilThree(points, language);
        trimToMax(points, 3);
        ensureCategoryCoverageByReplacement(points, language);
        return points;
    }

    private List<String> normalizeRecommendationPoints(FeedbackLanguage language) {
        List<String> rawPoints = collectRawPoints();
        return List.of(
                resolveFillerPoint(rawPoints, language),
                resolveAdjectivePoint(rawPoints, language),
                resolveAdverbPoint(rawPoints, language)
        );
    }

    private List<String> collectRawPoints() {
        List<String> rawPoints = new ArrayList<>();
        getCorrectionPoints().stream()
                .filter(p -> p != null && !p.isBlank())
                .map(String::trim)
                .forEach(rawPoints::add);
        getRecommendation().stream()
                .filter(p -> p != null && !p.isBlank())
                .map(String::trim)
                .forEach(rawPoints::add);
        return rawPoints;
    }

    private boolean isAuxiliaryPoint(String point) {
        return isFillerPoint(point) || isAdjectivePoint(point) || isAdverbPoint(point);
    }

    private String resolveFillerPoint(List<String> rawPoints, FeedbackLanguage language) {
        for (String point : rawPoints) {
            if (isFillerPoint(point)) {
                return normalizeFillerPoint(point);
            }
        }
        return defaultFillerPoint(language);
    }

    private boolean isFillerPoint(String point) {
        if (point == null) {
            return false;
        }
        String lowered = point.toLowerCase(Locale.ROOT);
        return lowered.contains("filler") || lowered.contains("필러");
    }

    private String normalizeFillerPoint(String point) {
        String normalized = point == null ? "" : point.trim();
        if (normalized.isEmpty()) {
            return "Filler:";
        }
        if (normalized.toLowerCase(Locale.ROOT).startsWith("filler:")) {
            return normalized;
        }
        if (normalized.startsWith("필러:")) {
            String body = normalized.substring("필러:".length()).trim();
            return body.isEmpty() ? "Filler:" : "Filler: " + body;
        }
        return "Filler: " + normalized;
    }

    private String defaultFillerPoint(FeedbackLanguage language) {
        if (language != null && language.isEnglish()) {
            return "Filler: Well - Use it at the beginning of a sentence to buy a short moment before your main point.";
        }
        return "Filler: Well - 문장 시작에서 생각을 정리할 짧은 시간을 벌 때 자연스럽게 사용하세요.";
    }

    private String resolveAdjectivePoint(List<String> rawPoints, FeedbackLanguage language) {
        for (String point : rawPoints) {
            if (isAdjectivePoint(point)) {
                return normalizeAdjectivePoint(point);
            }
        }
        return defaultAdjectivePoint(language);
    }

    private String resolveAdverbPoint(List<String> rawPoints, FeedbackLanguage language) {
        for (String point : rawPoints) {
            if (isAdverbPoint(point)) {
                return normalizeAdverbPoint(point);
            }
        }
        return defaultAdverbPoint(language);
    }

    private boolean isAdjectivePoint(String point) {
        if (point == null) {
            return false;
        }
        String lowered = point.toLowerCase(Locale.ROOT);
        return lowered.contains("adjective") || lowered.contains("형용사");
    }

    private boolean isAdverbPoint(String point) {
        if (point == null) {
            return false;
        }
        String lowered = point.toLowerCase(Locale.ROOT);
        return lowered.contains("adverb") || lowered.contains("부사");
    }

    private String normalizeAdjectivePoint(String point) {
        return normalizePrefixedPoint(point, "Adjective:", "형용사:");
    }

    private String normalizeAdverbPoint(String point) {
        return normalizePrefixedPoint(point, "Adverb:", "부사:");
    }

    private String normalizePrefixedPoint(String point, String englishPrefix, String koreanPrefix) {
        String normalized = point == null ? "" : point.trim();
        if (normalized.isEmpty()) {
            return englishPrefix;
        }
        if (normalized.toLowerCase(Locale.ROOT).startsWith(englishPrefix.toLowerCase(Locale.ROOT))) {
            return normalized;
        }
        if (normalized.startsWith(koreanPrefix)) {
            String body = normalized.substring(koreanPrefix.length()).trim();
            return body.isEmpty() ? englishPrefix : englishPrefix + " " + body;
        }
        return englishPrefix + " " + normalized;
    }

    private String defaultAdjectivePoint(FeedbackLanguage language) {
        if (language != null && language.isEnglish()) {
            return "Adjective: impressive - Use it to describe your experience more vividly.";
        }
        return "Adjective: vivid - 상황을 더 생생하게 설명할 때 사용하세요.";
    }

    private String defaultAdverbPoint(FeedbackLanguage language) {
        if (language != null && language.isEnglish()) {
            return "Adverb: definitely - Use it to emphasize certainty naturally.";
        }
        return "Adverb: clearly - 의견의 확신도를 자연스럽게 강조할 때 사용하세요.";
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

    private static Corrections toCorrections(List<String> points) {
        List<String> safe = points == null ? List.of() : points;
        return new Corrections(
                toCorrectionDetail(resolvePointForCategory(safe, "Grammar", 0), "Grammar"),
                toCorrectionDetail(resolvePointForCategory(safe, "Expression", 1), "Expression"),
                toCorrectionDetail(resolvePointForCategory(safe, "Logic", 2), "Logic")
        );
    }

    private static Recommendations toRecommendations(List<String> points) {
        List<String> safe = points == null ? List.of() : points;
        return new Recommendations(
                toRecommendationDetail(resolvePointForCategory(safe, "Filler", 0), "Filler"),
                toRecommendationDetail(resolvePointForCategory(safe, "Adjective", 1), "Adjective"),
                toRecommendationDetail(resolvePointForCategory(safe, "Adverb", 2), "Adverb")
        );
    }

    private static List<String> mergePoints(List<String> first, List<String> second) {
        List<String> merged = new ArrayList<>();
        if (first != null) {
            merged.addAll(first);
        }
        if (second != null) {
            merged.addAll(second);
        }
        return merged;
    }

    private static String resolvePointForCategory(List<String> points, String category, int fallbackIndex) {
        String matched = findCategoryPoint(points, category);
        if (!matched.isBlank()) {
            return matched;
        }
        if (fallbackIndex >= 0 && fallbackIndex < points.size()) {
            String fallback = points.get(fallbackIndex);
            return fallback == null ? "" : fallback;
        }
        return "";
    }

    private static String findCategoryPoint(List<String> points, String category) {
        for (String point : points) {
            if (matchesCategory(point, category)) {
                return point;
            }
        }
        return "";
    }

    private static boolean matchesCategory(String point, String category) {
        if (point == null || category == null) {
            return false;
        }
        String lowered = point.toLowerCase(Locale.ROOT);
        return switch (category) {
            case "Grammar" -> lowered.contains("grammar") || lowered.contains("문법");
            case "Expression" -> lowered.contains("expression") || lowered.contains("표현");
            case "Logic" -> lowered.contains("logic") || lowered.contains("논리");
            case "Filler" -> lowered.contains("filler") || lowered.contains("필러");
            case "Adjective" -> lowered.contains("adjective") || lowered.contains("형용사");
            case "Adverb" -> lowered.contains("adverb") || lowered.contains("부사");
            default -> false;
        };
    }

    private static CorrectionDetail toCorrectionDetail(String point, String category) {
        String body = stripCategoryPrefix(point, category);
        String[] parts = splitByDelimiter(body);
        String issue = parts[0].trim();
        String fix = parts.length > 1 ? parts[1].trim() : "";
        return new CorrectionDetail(issue, fix);
    }

    private static RecommendationDetail toRecommendationDetail(String point, String category) {
        String body = stripCategoryPrefix(point, category);
        String[] parts = splitByDelimiter(body);
        String term = parts[0].trim();
        String usage = parts.length > 1 ? parts[1].trim() : "";
        return new RecommendationDetail(term, usage);
    }

    private static String stripCategoryPrefix(String text, String category) {
        if (text == null) {
            return "";
        }
        String trimmed = text.trim();
        if (trimmed.isEmpty()) {
            return "";
        }
        String prefix = category + ":";
        if (trimmed.toLowerCase(Locale.ROOT).startsWith(prefix.toLowerCase(Locale.ROOT))) {
            return trimmed.substring(prefix.length()).trim();
        }
        return trimmed;
    }

    private static String[] splitByDelimiter(String text) {
        if (text == null || text.isBlank()) {
            return new String[]{""};
        }
        int index = text.indexOf(" - ");
        if (index > 0) {
            return new String[]{text.substring(0, index), text.substring(index + 3)};
        }
        index = text.indexOf(" | ");
        if (index > 0) {
            return new String[]{text.substring(0, index), text.substring(index + 3)};
        }
        return new String[]{text};
    }

    private String formatCorrection(String category, CorrectionDetail detail) {
        if (detail == null) {
            return category + ":";
        }
        String issue = detail.issue();
        String fix = detail.fix();
        if (!issue.isBlank() && !fix.isBlank()) {
            return category + ": " + issue + " - " + fix;
        }
        if (!issue.isBlank()) {
            return category + ": " + issue;
        }
        if (!fix.isBlank()) {
            return category + ": " + fix;
        }
        return category + ":";
    }

    private String formatRecommendation(String category, RecommendationDetail detail) {
        if (detail == null) {
            return category + ":";
        }
        String term = detail.term();
        String usage = detail.usage();
        if (!term.isBlank() && !usage.isBlank()) {
            return category + ": " + term + " - " + usage;
        }
        if (!term.isBlank()) {
            return category + ": " + term;
        }
        if (!usage.isBlank()) {
            return category + ": " + usage;
        }
        return category + ":";
    }

    public String getSummary() {
        return summary;
    }

    public Corrections getCorrections() {
        return corrections;
    }

    public Recommendations getRecommendations() {
        return recommendations;
    }

    public List<String> getCorrectionPoints() {
        List<String> points = List.of(
                formatCorrection("Grammar", corrections.grammar()),
                formatCorrection("Expression", corrections.expression()),
                formatCorrection("Logic", corrections.logic())
        );
        return Collections.unmodifiableList(points);
    }

    public List<String> getRecommendation() {
        List<String> points = List.of(
                formatRecommendation("Filler", recommendations.filler()),
                formatRecommendation("Adjective", recommendations.adjective()),
                formatRecommendation("Adverb", recommendations.adverb())
        );
        return Collections.unmodifiableList(points);
    }

    public String getExampleAnswer() {
        return exampleAnswer;
    }

    public List<String> getRulebookEvidence() {
        return Collections.unmodifiableList(rulebookEvidence);
    }
}
