use omypic_domain::feedback::{
    CorrectionDetail, Corrections, Feedback, RecommendationDetail, Recommendations,
};
use serde_json::Value;

/// LLM이 돌려준 피드백 JSON 문서를 도메인 모델로 옮긴다.
/// 구조화된 형태(corrections/recommendations 객체)와 예전의 평평한 형태
/// (correctionPoints/recommendation 배열)를 모두 받는다.
pub struct FeedbackJsonParser;

impl FeedbackJsonParser {
    pub fn parse(root: &Value) -> Feedback {
        let summary = as_text(path(root, "summary"), "");
        let example_answer = as_text(path(root, "exampleAnswer"), "");
        let evidence = to_string_list(path(root, "rulebookEvidence"));
        let corrections = path(root, "corrections");
        let recommendations = path(root, "recommendations");

        if !corrections.is_object() && !recommendations.is_object() {
            return parse_flat(root, &summary, &example_answer, &evidence);
        }
        Feedback::of(
            &summary,
            parse_corrections(corrections),
            parse_recommendations(recommendations),
            &example_answer,
            &evidence,
        )
    }

    /// 앞뒤에 글이 붙은 응답에서 가장 바깥 JSON 객체를 잘라 낸다.
    pub fn extract_json(raw: &str) -> String {
        if omypic_domain::shared::text_utils::java_is_blank(raw) {
            return "{}".to_string();
        }
        if let (Some(start), Some(end)) = (raw.find('{'), raw.rfind('}')) {
            if end > start {
                return raw[start..=end].to_string();
            }
        }
        raw.to_string()
    }
}

fn parse_flat(root: &Value, summary: &str, example_answer: &str, evidence: &[String]) -> Feedback {
    let correction_points = to_string_list(path(root, "correctionPoints"));
    let recommendation = to_string_list(path(root, "recommendation"));
    Feedback::of_points(
        summary,
        &correction_points,
        &recommendation,
        example_answer,
        evidence,
    )
}

fn parse_corrections(node: &Value) -> Corrections {
    Corrections::new(
        parse_correction_detail(path(node, "grammar")),
        parse_correction_detail(path(node, "expression")),
        parse_correction_detail(path(node, "logic")),
    )
}

fn parse_recommendations(node: &Value) -> Recommendations {
    Recommendations::new(
        parse_recommendation_detail(path(node, "filler")),
        parse_recommendation_detail(path(node, "adjective")),
        parse_recommendation_detail(path(node, "adverb")),
    )
}

fn parse_correction_detail(node: &Value) -> CorrectionDetail {
    if !node.is_object() {
        return CorrectionDetail::new("", "");
    }
    CorrectionDetail::new(
        &as_text(path(node, "issue"), ""),
        &as_text(path(node, "fix"), ""),
    )
}

fn parse_recommendation_detail(node: &Value) -> RecommendationDetail {
    if !node.is_object() {
        return RecommendationDetail::new("", "");
    }
    RecommendationDetail::new(
        &as_text(path(node, "term"), ""),
        &as_text(path(node, "usage"), ""),
    )
}

fn to_string_list(node: &Value) -> Vec<String> {
    match node {
        Value::Array(items) => items.iter().map(|item| as_text(item, "")).collect(),
        _ => Vec::new(),
    }
}

/// Jackson `JsonNode.path(name)`: 없으면 null 노드(`Value::Null`)를 돌려준다.
pub(crate) fn path<'a>(node: &'a Value, key: &str) -> &'a Value {
    node.get(key).unwrap_or(&Value::Null)
}

/// Jackson `JsonNode.path(index)`.
pub(crate) fn path_index(node: &Value, index: usize) -> &Value {
    node.get(index).unwrap_or(&Value::Null)
}

/// Jackson `JsonNode.asText(default)`: 문자열은 그대로, 숫자와 불리언은 글자로,
/// null/없음은 기본값, 배열과 객체는 빈 문자열.
pub(crate) fn as_text(node: &Value, default: &str) -> String {
    match node {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        Value::Bool(flag) => flag.to_string(),
        Value::Null => default.to_string(),
        Value::Array(_) | Value::Object(_) => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_reads_structured_shape() {
        let root = json!({
            "summary": "요약",
            "corrections": {
                "grammar": {"issue": "g-issue", "fix": "g-fix"},
                "expression": {"issue": "e-issue", "fix": "e-fix"},
                "logic": {"issue": "l-issue", "fix": "l-fix"}
            },
            "recommendations": {
                "filler": {"term": "Well", "usage": "f-usage"},
                "adjective": {"term": "vivid", "usage": "a-usage"},
                "adverb": {"term": "definitely", "usage": "d-usage"}
            },
            "exampleAnswer": "example",
            "rulebookEvidence": ["[a.md] evidence"]
        });

        let feedback = FeedbackJsonParser::parse(&root);

        assert_eq!("요약", feedback.summary());
        assert_eq!("g-issue", feedback.corrections().grammar().issue());
        assert_eq!("e-fix", feedback.corrections().expression().fix());
        assert_eq!("l-issue", feedback.corrections().logic().issue());
        assert_eq!("Well", feedback.recommendations().filler().term());
        assert_eq!("d-usage", feedback.recommendations().adverb().usage());
        assert_eq!("example", feedback.example_answer());
        assert_eq!(["[a.md] evidence"], feedback.rulebook_evidence());
    }

    #[test]
    fn parse_fills_missing_structured_members_with_empty_details() {
        let root = json!({"summary": "s", "corrections": {"grammar": {"issue": "only"}}});

        let feedback = FeedbackJsonParser::parse(&root);

        assert_eq!("only", feedback.corrections().grammar().issue());
        assert_eq!("", feedback.corrections().grammar().fix());
        assert_eq!("", feedback.corrections().logic().issue());
        assert_eq!("", feedback.recommendations().filler().term());
    }

    #[test]
    fn parse_reads_flat_shape_when_no_structured_objects() {
        let root = json!({
            "summary": "s",
            "correctionPoints": ["Grammar: tense", "Expression: verbs"],
            "recommendation": ["Filler: well"],
            "exampleAnswer": "ex",
            "rulebookEvidence": []
        });

        let feedback = FeedbackJsonParser::parse(&root);

        let expected = Feedback::of_points(
            "s",
            &[
                "Grammar: tense".to_string(),
                "Expression: verbs".to_string(),
            ],
            &["Filler: well".to_string()],
            "ex",
            &[],
        );
        assert_eq!(expected, feedback);
    }

    #[test]
    fn parse_uses_as_text_rules_for_non_string_values() {
        let root = json!({"summary": 5, "exampleAnswer": null, "rulebookEvidence": [true, null, {"a": 1}, "x"]});

        let feedback = FeedbackJsonParser::parse(&root);

        assert_eq!("5", feedback.summary());
        assert_eq!("", feedback.example_answer());
        assert_eq!(["true", "", "", "x"], feedback.rulebook_evidence());
    }

    #[test]
    fn extract_json_cuts_outermost_object() {
        assert_eq!(
            "{\"a\":{\"b\":1}}",
            FeedbackJsonParser::extract_json("noise {\"a\":{\"b\":1}} tail")
        );
        assert_eq!("{}", FeedbackJsonParser::extract_json("  "));
        assert_eq!("plain", FeedbackJsonParser::extract_json("plain"));
        assert_eq!("} {", FeedbackJsonParser::extract_json("} {"));
    }
}
