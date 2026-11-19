use super::{
    CorrectionDetail, Corrections, FeedbackConstraints, FeedbackLanguage, RecommendationDetail,
    Recommendations,
};
use crate::rulebook::RulebookContext;
use crate::shared::text_utils::{
    java_is_blank, java_trim, trim_to_length, utf16_len, utf16_prefix,
};

const CATEGORY_ORDER: [&str; 3] = ["Grammar", "Expression", "Logic"];
const MAX_CORRECTION_POINTS: usize = 3;
const MAX_EVIDENCE_TEXT_CHARS: usize = 200;

/// LLM이 만든 피드백. 길이와 개수 규칙은 `normalized`가 적용하며, 모든 길이는 Java처럼
/// UTF-16 코드 유닛으로 센다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Feedback {
    summary: String,
    corrections: Corrections,
    recommendations: Recommendations,
    example_answer: String,
    rulebook_evidence: Vec<String>,
}

impl Feedback {
    pub fn of(
        summary: &str,
        corrections: Corrections,
        recommendations: Recommendations,
        example_answer: &str,
        rulebook_evidence: &[String],
    ) -> Self {
        Self {
            summary: summary.to_string(),
            corrections,
            recommendations,
            example_answer: example_answer.to_string(),
            rulebook_evidence: rulebook_evidence.to_vec(),
        }
    }

    /// 교정 포인트와 추천 포인트를 합쳐서 추천 항목을 찾는 Java `of(…, correctionPoints, recommendation, …)`.
    pub fn of_points(
        summary: &str,
        correction_points: &[String],
        recommendation: &[String],
        example_answer: &str,
        rulebook_evidence: &[String],
    ) -> Self {
        let merged: Vec<String> = correction_points
            .iter()
            .chain(recommendation.iter())
            .cloned()
            .collect();
        Self::of(
            summary,
            to_corrections(correction_points),
            to_recommendations(&merged),
            example_answer,
            rulebook_evidence,
        )
    }

    /// 교정 포인트만 받는 Java `of(summary, correctionPoints, exampleAnswer, rulebookEvidence)`.
    pub fn of_correction_points(
        summary: &str,
        correction_points: &[String],
        example_answer: &str,
        rulebook_evidence: &[String],
    ) -> Self {
        Self::of(
            summary,
            to_corrections(correction_points),
            to_recommendations(correction_points),
            example_answer,
            rulebook_evidence,
        )
    }

    pub fn normalized(
        &self,
        constraints: &FeedbackConstraints,
        user_text: &str,
        language: &FeedbackLanguage,
        contexts: &[RulebookContext],
    ) -> Feedback {
        let summary = self.normalize_summary(language, constraints.summary_max_chars);
        let points = self.normalize_correction_points(language);
        let recommendation_points = self.normalize_recommendation_points();
        let example = self.normalize_example_answer(
            user_text,
            language,
            constraints.example_min_ratio,
            constraints.example_max_ratio,
        );
        let evidence = self.normalize_rulebook_evidence(contexts);
        Feedback::of(
            &summary,
            to_corrections(&points),
            to_recommendations(&recommendation_points),
            &example,
            &evidence,
        )
    }

    pub fn summary(&self) -> &str {
        &self.summary
    }

    pub fn corrections(&self) -> &Corrections {
        &self.corrections
    }

    pub fn recommendations(&self) -> &Recommendations {
        &self.recommendations
    }

    pub fn correction_points(&self) -> Vec<String> {
        vec![
            format_correction("Grammar", self.corrections.grammar()),
            format_correction("Expression", self.corrections.expression()),
            format_correction("Logic", self.corrections.logic()),
        ]
    }

    pub fn recommendation(&self) -> Vec<String> {
        vec![
            format_recommendation("Filler", self.recommendations.filler()),
            format_recommendation("Adjective", self.recommendations.adjective()),
            format_recommendation("Adverb", self.recommendations.adverb()),
        ]
    }

    pub fn example_answer(&self) -> &str {
        &self.example_answer
    }

    pub fn rulebook_evidence(&self) -> &[String] {
        &self.rulebook_evidence
    }

    fn normalize_summary(&self, language: &FeedbackLanguage, max_chars: usize) -> String {
        let trimmed = java_trim(&self.summary);
        let base = if trimmed.is_empty() {
            default_summary(language)
        } else {
            trimmed.to_string()
        };
        let sentences = split_sentences(&base);
        let selected = match sentences.as_slice() {
            [] => default_summary(language),
            [only] => only.clone(),
            [first, second, ..] => format!("{first} {second}"),
        };
        let trimmed = java_trim(&trim_to_length(Some(&selected), max_chars)).to_string();
        if !trimmed.is_empty() {
            return trimmed;
        }
        java_trim(&trim_to_length(Some(&default_summary(language)), max_chars)).to_string()
    }

    fn normalize_correction_points(&self, language: &FeedbackLanguage) -> Vec<String> {
        let mut points: Vec<String> = self
            .collect_raw_points()
            .into_iter()
            .filter(|point| !is_auxiliary_point(point))
            .collect();
        points.truncate(MAX_CORRECTION_POINTS);
        append_missing_categories(&mut points, language);
        fill_until_three(&mut points, language);
        points.truncate(MAX_CORRECTION_POINTS);
        ensure_category_coverage_by_replacement(&mut points, language);
        points
    }

    fn normalize_recommendation_points(&self) -> Vec<String> {
        let raw = self.collect_raw_points();
        vec![
            resolve_prefixed_point(&raw, is_filler_point, "Filler:", "필러:"),
            resolve_prefixed_point(&raw, is_adjective_point, "Adjective:", "형용사:"),
            resolve_prefixed_point(&raw, is_adverb_point, "Adverb:", "부사:"),
        ]
    }

    fn collect_raw_points(&self) -> Vec<String> {
        self.correction_points()
            .into_iter()
            .chain(self.recommendation())
            .filter(|p| !java_is_blank(p))
            .map(|p| java_trim(&p).to_string())
            .collect()
    }

    fn normalize_example_answer(
        &self,
        user_text: &str,
        language: &FeedbackLanguage,
        min_ratio: f64,
        max_ratio: f64,
    ) -> String {
        let mut normalized = java_trim(&self.example_answer).to_string();
        let user_length = utf16_len(java_trim(user_text)) as i32;
        if user_length <= 0 {
            if !normalized.is_empty() {
                return normalized;
            }
            return default_example_seed(language, user_text);
        }

        let min_length = ((user_length as f64 * min_ratio).ceil() as i32).max(1);
        let max_length = ((user_length as f64 * max_ratio).floor() as i32).max(min_length);
        let (min_length, max_length) = (min_length as usize, max_length as usize);
        if normalized.is_empty() {
            normalized = default_example_seed(language, user_text);
        }
        if utf16_len(&normalized) > max_length {
            normalized = cut(&normalized, max_length);
        }
        let english = language.is_english();
        let filler = if english {
            " Add one concrete detail."
        } else {
            " 구체적인 디테일을 한 문장 더 추가하세요."
        };
        while utf16_len(&normalized) < min_length {
            normalized = java_trim(&format!("{normalized}{filler}")).to_string();
            if utf16_len(&normalized) > max_length {
                normalized = cut(&normalized, max_length);
                break;
            }
        }
        if utf16_len(&normalized) < min_length {
            let fallback = java_trim(user_text);
            if !fallback.is_empty() {
                normalized = fallback.to_string();
            }
            if utf16_len(&normalized) > max_length {
                normalized = cut(&normalized, max_length);
            }
            while utf16_len(&normalized) < min_length {
                normalized.push_str(if english { " detail" } else { " 디테일" });
                if utf16_len(&normalized) > max_length {
                    normalized = cut(&normalized, max_length);
                    break;
                }
            }
        }
        normalized
    }

    fn normalize_rulebook_evidence(&self, contexts: &[RulebookContext]) -> Vec<String> {
        let Some(first) = contexts.first() else {
            return Vec::new();
        };
        let cleaned: Vec<String> = self
            .rulebook_evidence
            .iter()
            .filter(|e| !java_is_blank(e))
            .map(|e| java_trim(e).to_string())
            .collect();
        if !cleaned.is_empty() {
            return cleaned;
        }
        vec![format!(
            "[{}] {}",
            first.filename(),
            trim_to_length(Some(first.text()), MAX_EVIDENCE_TEXT_CHARS)
        )]
    }
}

/// Java `substring(0, max).trim()`.
fn cut(text: &str, max_units: usize) -> String {
    java_trim(utf16_prefix(text, max_units)).to_string()
}

/// Java `text.split("(?<=[.!?])\\s+|\\n+")` 뒤에 `trim`과 빈 조각 제거를 적용한 결과.
/// 정규식의 `\s`는 ASCII 공백(`[ \t\n\x0B\f\r]`)만 뜻한다.
fn split_sentences(text: &str) -> Vec<String> {
    fn is_regex_space(c: char) -> bool {
        matches!(c, ' ' | '\t' | '\n' | '\u{000B}' | '\u{000C}' | '\r')
    }
    fn run_end(chars: &[char], from: usize, accept: impl Fn(char) -> bool) -> usize {
        let mut end = from;
        while end < chars.len() && accept(chars[end]) {
            end += 1;
        }
        end
    }

    let chars: Vec<char> = text.chars().collect();
    let mut parts: Vec<String> = Vec::new();
    let mut start = 0;
    let mut index = 0;
    while index < chars.len() {
        let c = chars[index];
        let after_sentence_end = index > 0 && matches!(chars[index - 1], '.' | '!' | '?');
        let separator_end = if is_regex_space(c) && after_sentence_end {
            Some(run_end(&chars, index, is_regex_space))
        } else if c == '\n' {
            Some(run_end(&chars, index, |c| c == '\n'))
        } else {
            None
        };
        match separator_end {
            Some(end) => {
                parts.push(chars[start..index].iter().collect());
                start = end;
                index = end;
            }
            None => index += 1,
        }
    }
    parts.push(chars[start..].iter().collect());
    parts
        .into_iter()
        .map(|s| java_trim(&s).to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn contains_any(point: &str, needles: [&str; 2]) -> bool {
    let lowered = point.to_lowercase();
    needles.iter().any(|needle| lowered.contains(needle))
}

fn is_filler_point(point: &str) -> bool {
    contains_any(point, ["filler", "필러"])
}

fn is_adjective_point(point: &str) -> bool {
    contains_any(point, ["adjective", "형용사"])
}

fn is_adverb_point(point: &str) -> bool {
    contains_any(point, ["adverb", "부사"])
}

fn is_auxiliary_point(point: &str) -> bool {
    is_filler_point(point) || is_adjective_point(point) || is_adverb_point(point)
}

fn resolve_prefixed_point(
    raw_points: &[String],
    matches: fn(&str) -> bool,
    english_prefix: &str,
    korean_prefix: &str,
) -> String {
    raw_points
        .iter()
        .find(|point| matches(point))
        .map(|point| normalize_prefixed_point(point, english_prefix, korean_prefix))
        .unwrap_or_else(|| english_prefix.to_string())
}

fn normalize_prefixed_point(point: &str, english_prefix: &str, korean_prefix: &str) -> String {
    let normalized = java_trim(point);
    if normalized.is_empty() {
        return english_prefix.to_string();
    }
    if normalized
        .to_lowercase()
        .starts_with(&english_prefix.to_lowercase())
    {
        return normalized.to_string();
    }
    if let Some(body) = normalized.strip_prefix(korean_prefix) {
        let body = java_trim(body);
        return if body.is_empty() {
            english_prefix.to_string()
        } else {
            format!("{english_prefix} {body}")
        };
    }
    format!("{english_prefix} {normalized}")
}

fn detect_category(point: &str) -> Option<&'static str> {
    if contains_any(point, ["grammar", "문법"]) {
        Some("Grammar")
    } else if contains_any(point, ["expression", "표현"]) {
        Some("Expression")
    } else if contains_any(point, ["logic", "논리"]) {
        Some("Logic")
    } else {
        None
    }
}

/// 입력 순서를 유지하고 중복이 없는 카테고리 목록 (Java `LinkedHashSet`).
fn categories_of(points: &[String]) -> Vec<&'static str> {
    let mut categories = Vec::new();
    for point in points {
        if let Some(category) = detect_category(point) {
            if !categories.contains(&category) {
                categories.push(category);
            }
        }
    }
    categories
}

fn append_missing_categories(points: &mut Vec<String>, language: &FeedbackLanguage) {
    let mut categories = categories_of(points);
    for category in CATEGORY_ORDER {
        if categories.len() >= 2 {
            return;
        }
        if categories.contains(&category) {
            continue;
        }
        points.push(default_point_for_category(language, category));
        categories.push(category);
    }
}

fn fill_until_three(points: &mut Vec<String>, language: &FeedbackLanguage) {
    while points.len() < MAX_CORRECTION_POINTS {
        points.push(default_point(language, points.len()));
    }
}

fn ensure_category_coverage_by_replacement(points: &mut [String], language: &FeedbackLanguage) {
    if points.is_empty() || categories_of(points).len() >= 2 {
        return;
    }
    let last = points.len() - 1;
    for category in CATEGORY_ORDER {
        if categories_of(points).contains(&category) {
            continue;
        }
        points[last] = default_point_for_category(language, category);
        if categories_of(points).len() >= 2 {
            return;
        }
    }
}

fn default_summary(language: &FeedbackLanguage) -> String {
    if language.is_english() {
        "Your answer is understandable and can be improved with clearer structure.".to_string()
    } else {
        "답변의 핵심은 전달되었습니다. 구조를 더 선명하게 다듬으면 더 좋아집니다.".to_string()
    }
}

fn default_example_seed(language: &FeedbackLanguage, user_text: &str) -> String {
    let base = java_trim(user_text);
    if !base.is_empty() {
        return base.to_string();
    }
    if language.is_english() {
        "I can explain my idea clearly with one concrete example.".to_string()
    } else {
        "저는 핵심 의견을 한 가지 예시와 함께 분명하게 설명할 수 있습니다.".to_string()
    }
}

fn default_point_for_category(language: &FeedbackLanguage, category: &str) -> String {
    let english = language.is_english();
    match (category, english) {
        ("Grammar", true) => "Grammar: Check verb tense consistency.",
        ("Grammar", false) => "Grammar: 시제 일관성을 점검하세요.",
        ("Expression", true) => "Expression: Use more specific vocabulary.",
        ("Expression", false) => "Expression: 더 구체적인 어휘를 사용하세요.",
        (_, true) => "Logic: Add a clear reason or example.",
        (_, false) => "Logic: 이유나 예시를 추가하세요.",
    }
    .to_string()
}

fn default_point(language: &FeedbackLanguage, index: usize) -> String {
    let category = match index {
        0 => "Grammar",
        1 => "Expression",
        _ => "Logic",
    };
    default_point_for_category(language, category)
}

fn to_corrections(points: &[String]) -> Corrections {
    Corrections::new(
        to_correction_detail(&resolve_point_for_category(points, "Grammar", 0), "Grammar"),
        to_correction_detail(
            &resolve_point_for_category(points, "Expression", 1),
            "Expression",
        ),
        to_correction_detail(&resolve_point_for_category(points, "Logic", 2), "Logic"),
    )
}

fn to_recommendations(points: &[String]) -> Recommendations {
    Recommendations::new(
        to_recommendation_detail(&resolve_point_for_category(points, "Filler", 0), "Filler"),
        to_recommendation_detail(
            &resolve_point_for_category(points, "Adjective", 1),
            "Adjective",
        ),
        to_recommendation_detail(&resolve_point_for_category(points, "Adverb", 2), "Adverb"),
    )
}

fn resolve_point_for_category(points: &[String], category: &str, fallback_index: usize) -> String {
    points
        .iter()
        .find(|point| matches_category(point, category))
        .or_else(|| points.get(fallback_index))
        .cloned()
        .unwrap_or_default()
}

fn matches_category(point: &str, category: &str) -> bool {
    match category {
        "Grammar" => contains_any(point, ["grammar", "문법"]),
        "Expression" => contains_any(point, ["expression", "표현"]),
        "Logic" => contains_any(point, ["logic", "논리"]),
        "Filler" => is_filler_point(point),
        "Adjective" => is_adjective_point(point),
        "Adverb" => is_adverb_point(point),
        _ => false,
    }
}

fn to_correction_detail(point: &str, category: &str) -> CorrectionDetail {
    let (issue, fix) = split_by_delimiter(&strip_category_prefix(point, category));
    CorrectionDetail::new(&issue, &fix)
}

fn to_recommendation_detail(point: &str, category: &str) -> RecommendationDetail {
    let (term, usage) = split_by_delimiter(&strip_category_prefix(point, category));
    RecommendationDetail::new(&term, &usage)
}

fn strip_category_prefix(text: &str, category: &str) -> String {
    let trimmed = java_trim(text);
    if trimmed.is_empty() {
        return String::new();
    }
    let prefix = format!("{category}:");
    if trimmed.to_lowercase().starts_with(&prefix.to_lowercase()) {
        if let Some(rest) = trimmed.get(prefix.len()..) {
            return java_trim(rest).to_string();
        }
    }
    trimmed.to_string()
}

/// 첫 번째 ` - ` 또는 ` | `에서 둘로 나눈다. 구분자가 없으면 두 번째 값은 비어 있다.
fn split_by_delimiter(text: &str) -> (String, String) {
    if java_is_blank(text) {
        return (String::new(), String::new());
    }
    for delimiter in [" - ", " | "] {
        if let Some(index) = text.find(delimiter) {
            if index > 0 {
                return (
                    text[..index].to_string(),
                    text[index + delimiter.len()..].to_string(),
                );
            }
        }
    }
    (text.to_string(), String::new())
}

fn format_point(category: &str, first: &str, second: &str) -> String {
    match (
        first.is_empty() || java_is_blank(first),
        second.is_empty() || java_is_blank(second),
    ) {
        (false, false) => format!("{category}: {first} - {second}"),
        (false, true) => format!("{category}: {first}"),
        (true, false) => format!("{category}: {second}"),
        (true, true) => format!("{category}:"),
    }
}

fn format_correction(category: &str, detail: &CorrectionDetail) -> String {
    format_point(category, detail.issue(), detail.fix())
}

fn format_recommendation(category: &str, detail: &RecommendationDetail) -> String {
    format_point(category, detail.term(), detail.usage())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rulebook::RulebookId;

    fn strs(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    fn constraints() -> FeedbackConstraints {
        FeedbackConstraints::new(255, 0.8, 1.2)
    }

    fn three_points() -> Vec<String> {
        strs(&["Grammar", "Expression", "Logic"])
    }

    #[test]
    fn normalized_splits_correction_points_and_recommendation() {
        let feedback = Feedback::of_correction_points(
            "summary",
            &strs(&["Grammar: tense"]),
            "example text",
            &[],
        );

        let normalized = feedback.normalized(
            &constraints(),
            "user answer text",
            &FeedbackLanguage::of("en"),
            &[],
        );

        assert_eq!(3, normalized.correction_points().len());
        assert!(normalized.correction_points()[0].contains("Grammar"));
        assert!(normalized.correction_points()[1].contains("Expression"));
        assert!(normalized.correction_points()[2].contains("Logic"));
        assert_eq!(3, normalized.recommendation().len());
        assert!(normalized.recommendation()[0].contains("Filler:"));
        assert!(normalized.recommendation()[1].contains("Adjective:"));
        assert!(normalized.recommendation()[2].contains("Adverb:"));
    }

    #[test]
    fn normalized_trims_summary_to_max_chars() {
        let feedback =
            Feedback::of_correction_points("1234567890", &three_points(), "example", &[]);
        let constraints = FeedbackConstraints::new(5, 0.8, 1.2);

        let normalized =
            feedback.normalized(&constraints, "12345", &FeedbackLanguage::of("ko"), &[]);

        assert_eq!("1234", normalized.summary());
    }

    #[test]
    fn normalized_treats_control_character_summary_as_empty_like_java_trim() {
        let language = FeedbackLanguage::of("ko");
        let blank = Feedback::of_correction_points("", &three_points(), "example", &[]);
        let control =
            Feedback::of_correction_points("\u{0001} \u{0002}", &three_points(), "example", &[]);

        let expected = blank.normalized(&constraints(), "answer", &language, &[]);
        let actual = control.normalized(&constraints(), "answer", &language, &[]);

        assert_eq!(expected.summary(), actual.summary());
    }

    #[test]
    fn normalized_trims_example_when_ratio_is_too_large() {
        let feedback =
            Feedback::of_correction_points("summary", &three_points(), &"x".repeat(30), &[]);

        let normalized = feedback.normalized(
            &constraints(),
            "1234567890",
            &FeedbackLanguage::of("en"),
            &[],
        );

        assert!(utf16_len(normalized.example_answer()) <= 12);
    }

    #[test]
    fn normalized_appends_filler_when_example_too_short() {
        let feedback = Feedback::of_correction_points("summary", &three_points(), "짧다", &[]);

        let normalized = feedback.normalized(
            &constraints(),
            "충분히 긴 사용자 답변 텍스트",
            &FeedbackLanguage::of("ko"),
            &[],
        );

        assert!(utf16_len(normalized.example_answer()) >= 13);
        assert!(utf16_len(normalized.example_answer()) <= 19);
    }

    #[test]
    fn normalized_limits_summary_to_two_sentences() {
        let feedback = Feedback::of_correction_points(
            "One. Two. Three.",
            &three_points(),
            "example answer",
            &[],
        );

        let normalized = feedback.normalized(
            &constraints(),
            "user answer",
            &FeedbackLanguage::of("en"),
            &[],
        );

        assert_eq!("One. Two.", normalized.summary());
    }

    #[test]
    fn normalized_enforces_at_least_two_point_categories() {
        let feedback = Feedback::of_correction_points(
            "summary",
            &strs(&["Logic: weak", "Logic: vague", "Logic: thin"]),
            "example answer",
            &[],
        );

        let normalized = feedback.normalized(
            &constraints(),
            "user answer long enough",
            &FeedbackLanguage::of("en"),
            &[],
        );

        assert_eq!(3, normalized.correction_points().len());
        let has = |name: &str| {
            normalized
                .correction_points()
                .iter()
                .any(|p| p.contains(name))
        };
        let (has_grammar, has_expression, has_logic) =
            (has("Grammar"), has("Expression"), has("Logic"));
        assert!(
            (has_grammar && has_logic)
                || (has_expression && has_logic)
                || (has_grammar && has_expression)
        );
    }

    #[test]
    fn normalized_injects_rulebook_evidence_when_context_exists_and_evidence_is_empty() {
        let feedback =
            Feedback::of_correction_points("summary", &three_points(), "example answer", &[]);
        let context = RulebookContext::of(
            RulebookId::of("r1").unwrap(),
            Some("rulebook.md"),
            Some("Evidence text"),
        );

        let normalized = feedback.normalized(
            &constraints(),
            "user answer",
            &FeedbackLanguage::of("en"),
            &[context],
        );

        assert_eq!(1, normalized.rulebook_evidence().len());
        assert!(normalized.rulebook_evidence()[0].starts_with("[rulebook.md]"));
    }

    #[test]
    fn normalized_clears_evidence_when_context_is_empty() {
        let feedback = Feedback::of_correction_points(
            "summary",
            &three_points(),
            "example answer",
            &strs(&["old evidence"]),
        );

        let normalized = feedback.normalized(
            &constraints(),
            "user answer",
            &FeedbackLanguage::of("en"),
            &[],
        );

        assert!(normalized.rulebook_evidence().is_empty());
    }

    #[test]
    fn normalized_preserves_provided_recommendation_points() {
        let feedback = Feedback::of_correction_points(
            "summary",
            &strs(&[
                "Grammar",
                "Expression",
                "Logic",
                "Filler: Actually: Use this to add or correct a detail.",
                "Adjective: impressive - Use it for vivid detail.",
                "Adverb: definitely - Use it for confidence.",
            ]),
            "example answer",
            &[],
        );

        let normalized = feedback.normalized(
            &constraints(),
            "user answer",
            &FeedbackLanguage::of("en"),
            &[],
        );

        let recommendation = normalized.recommendation();
        assert!(recommendation
            .iter()
            .any(|p| p == "Filler: Actually: Use this to add or correct a detail."));
        assert!(recommendation
            .iter()
            .any(|p| p == "Adjective: impressive - Use it for vivid detail."));
        assert!(recommendation
            .iter()
            .any(|p| p == "Adverb: definitely - Use it for confidence."));
    }

    // "한글😀abc"는 Java에서 length() == 7이다. char 수(6)로 세면 아래 결과가 달라진다.
    const MIXED: &str = "한글😀abc";

    #[test]
    fn normalized_counts_summary_length_in_utf16_units() {
        let feedback =
            Feedback::of_correction_points("한글😀abcdef", &three_points(), "example", &[]);
        let constraints = FeedbackConstraints::new(6, 0.8, 1.2);

        let normalized =
            feedback.normalized(&constraints, "12345", &FeedbackLanguage::of("ko"), &[]);

        // Java: 길이 10 > 6 이므로 substring(0, 5) == "한글😀a"
        assert_eq!("한글😀a", normalized.summary());
    }

    #[test]
    fn normalized_counts_example_length_in_utf16_units() {
        let feedback =
            Feedback::of_correction_points("summary", &three_points(), "한글😀abcdefgh", &[]);

        let normalized =
            feedback.normalized(&constraints(), MIXED, &FeedbackLanguage::of("ko"), &[]);

        // Java: 사용자 길이 7 -> min ceil(5.6)=6, max floor(8.4)=8. substring(0, 8) == "한글😀abcd"
        assert_eq!("한글😀abcd", normalized.example_answer());
    }

    #[test]
    fn normalized_pads_a_short_emoji_example_up_to_the_utf16_range() {
        let feedback = Feedback::of_correction_points("summary", &three_points(), "😀", &[]);

        let normalized =
            feedback.normalized(&constraints(), MIXED, &FeedbackLanguage::of("ko"), &[]);

        // Java: "😀 구체적인 디테일을…"을 substring(0, 8) 후 trim == "😀 구체적인"
        assert_eq!("😀 구체적인", normalized.example_answer());
    }

    #[test]
    fn split_sentences_matches_java_regex_split() {
        assert_eq!(
            vec!["One.", "Two!", "Three?"],
            split_sentences("One. Two!  Three?")
        );
        assert_eq!(vec!["a b", "c"], split_sentences("a b\n\nc"));
        assert_eq!(vec!["1.5 liters"], split_sentences("1.5 liters"));
        assert_eq!(vec!["Hi.", "there"], split_sentences("Hi.\n there\n"));
        assert!(split_sentences("  \n ").is_empty());
    }

    #[test]
    fn feedback_language_defaults_to_ko() {
        assert_eq!("ko", FeedbackLanguage::of("").value());
        assert_eq!("ko", FeedbackLanguage::of("  ").value());
        assert_eq!("ko", FeedbackLanguage::of("fr").value());
        assert_eq!("en", FeedbackLanguage::of("EN").value());
        assert!(FeedbackLanguage::of("en").is_english());
        assert!(!FeedbackLanguage::default().is_english());
    }
}
