//! 피드백과 추천 표현 요청에 쓰는 시스템/사용자 프롬프트를 만든다.

use omypic_domain::rulebook::RulebookContext;
use omypic_domain::shared::text_utils::{java_is_blank, java_trim};

use super::recommendation::RecommendationHistory;
use super::templates::{
    BASE_COACH_PROMPT_TEMPLATE, RECOMMENDATION_SYSTEM_PROMPT_TEMPLATE,
    RECOMMENDATION_USER_PROMPT_TEMPLATE, STRUCTURED_OUTPUT_INSTRUCTION_TEMPLATE,
    TURN_USER_PROMPT_TEMPLATE,
};

const NO_STRATEGY: &str = "(등록된 전략/룰북 문서가 없습니다)";
const NO_RULEBOOK_DOC: &str = "(문서 없음)";
const UNKNOWN_QUESTION: &str = "(질문 없음)";
const NO_RECENT_RECOMMENDATIONS: &str = "(없음)";

/// 프롬프트에 넣을 입력. 턴 입력(질문과 답변)이거나 일반 입력(텍스트 하나)이다.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct PromptInput {
    pub(super) question_text: String,
    pub(super) answer_text: String,
    pub(super) general_text: String,
}

impl PromptInput {
    pub(super) fn from_general(text: &str) -> Self {
        Self {
            general_text: trim_text(text),
            ..Self::default()
        }
    }

    pub(super) fn from_turn(question_text: &str, answer_text: &str) -> Self {
        Self {
            question_text: trim_text(question_text),
            answer_text: trim_text(answer_text),
            general_text: String::new(),
        }
    }

    fn is_turn_prompt(&self) -> bool {
        !java_is_blank(&self.question_text) || !java_is_blank(&self.answer_text)
    }

    /// 피드백 정규화(예시 답변 길이 등)의 기준이 되는 사용자 텍스트.
    pub(super) fn normalization_text(&self) -> &str {
        if self.is_turn_prompt() {
            &self.answer_text
        } else {
            &self.general_text
        }
    }
}

/// Java `text == null ? "" : text.trim()`.
pub(super) fn trim_text(text: &str) -> String {
    java_trim(text).to_string()
}

/// Java `String.formatted`의 `%s`와 `%%`만 다룬다. 인자 안의 `%`는 다시 해석하지 않는다.
pub(super) fn java_format(template: &str, args: &[&str]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut args = args.iter();
    let mut chars = template.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('s') => out.push_str(args.next().expect("missing format argument")),
            Some('%') => out.push('%'),
            other => panic!("unsupported format specifier: %{other:?}"),
        }
    }
    out
}

pub(super) fn build_system_prompt(language: &str, contexts: &[RulebookContext]) -> String {
    java_format(BASE_COACH_PROMPT_TEMPLATE, &[&render_strategy(contexts)])
        + "\n\n"
        + &structured_output_instruction(prompt_language(language))
}

pub(super) fn build_recommendation_system_prompt(
    language: &str,
    contexts: &[RulebookContext],
    strict_deduplication: bool,
) -> String {
    let duplicate_guideline = if strict_deduplication {
        "최근 추천어와 같은 term은 피하고, 특히 이미 중복된 항목은 다른 표현으로 교체하라."
    } else {
        "최근 추천어와의 중복을 줄이되, 현재 문맥에 가장 적합하면 중복을 허용해도 된다."
    };
    java_format(
        RECOMMENDATION_SYSTEM_PROMPT_TEMPLATE,
        &[&render_strategy(contexts), duplicate_guideline],
    ) + "\n\n"
        + &structured_output_instruction(prompt_language(language))
}

pub(super) fn build_recommendation_user_prompt(
    question_text: &str,
    answer_text: &str,
    contexts: &[RulebookContext],
    history: &RecommendationHistory,
) -> String {
    let mut safe_question = trim_text(question_text);
    if java_is_blank(&safe_question) {
        safe_question = UNKNOWN_QUESTION.to_string();
    }
    java_format(
        RECOMMENDATION_USER_PROMPT_TEMPLATE,
        &[
            &safe_question,
            &trim_text(answer_text),
            &rulebook_doc(contexts, 0),
            &rulebook_doc(contexts, 1),
            &join_history(&history.filler_terms),
            &join_history(&history.adjective_terms),
            &join_history(&history.adverb_terms),
        ],
    )
}

pub(super) fn build_user_prompt(
    input: &PromptInput,
    contexts: &[RulebookContext],
    language: &str,
) -> String {
    if input.is_turn_prompt() {
        return build_turn_prompt(&input.question_text, &input.answer_text, contexts);
    }
    let label = if language.eq_ignore_ascii_case("en") {
        "User answer"
    } else {
        "사용자 답변"
    };
    format!("{label}:\n{}", input.normalization_text())
}

pub(super) fn build_turn_prompt(
    question_text: &str,
    answer_text: &str,
    contexts: &[RulebookContext],
) -> String {
    let first_doc = rulebook_doc(contexts, 0);
    let second_doc = rulebook_doc(contexts, 1);
    java_format(
        TURN_USER_PROMPT_TEMPLATE,
        &[question_text, answer_text, &first_doc, &second_doc],
    )
}

/// 룰북 검색에 쓰는 질의. 질문과 답변이 모두 있으면 줄바꿈으로 잇는다.
pub(super) fn build_turn_query(question_text: &str, answer_text: &str) -> String {
    let q = java_trim(question_text);
    let a = java_trim(answer_text);
    if !java_is_blank(q) && !java_is_blank(a) {
        return format!("{q}\n{a}");
    }
    if !java_is_blank(a) {
        return a.to_string();
    }
    q.to_string()
}

fn prompt_language(language: &str) -> &'static str {
    if language.eq_ignore_ascii_case("en") {
        "English"
    } else {
        "Korean"
    }
}

fn structured_output_instruction(language: &str) -> String {
    java_format(STRUCTURED_OUTPUT_INSTRUCTION_TEMPLATE, &[language])
}

fn render_strategy(contexts: &[RulebookContext]) -> String {
    if contexts.is_empty() {
        return NO_STRATEGY.to_string();
    }
    contexts
        .iter()
        .map(|context| format!("- [{}] {}", context.filename(), context.text()))
        .collect::<Vec<_>>()
        .join("\n")
}

fn rulebook_doc(contexts: &[RulebookContext], index: usize) -> String {
    match contexts.get(index) {
        Some(context) => format!("[{}] {}", context.filename(), context.text()),
        None => NO_RULEBOOK_DOC.to_string(),
    }
}

fn join_history(terms: &[String]) -> String {
    if terms.is_empty() {
        return NO_RECENT_RECOMMENDATIONS.to_string();
    }
    terms.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_format_replaces_placeholders_and_unescapes_percent() {
        assert_eq!(
            "a 35% x%sy b",
            java_format("a 35%% %s b", &["x%sy"]),
            "an argument containing %s is not formatted again"
        );
    }

    #[test]
    fn base_coach_prompt_renders_percent_weights_like_java() {
        let prompt = build_system_prompt("ko", &[]);

        assert!(prompt.starts_with("지금부터 너는 OPIc(Oral Proficiency Interview Computer)"));
        assert!(prompt.contains("Function 35%, Content&Context 35%, Text Type 20%, Accuracy 10%\n"));
        assert!(prompt.contains("\n(등록된 전략/룰북 문서가 없습니다)\n"));
        assert!(prompt.contains("이제 본격적으로 학습을 도와라.\n\n\n반드시 JSON 객체 1개만"));
        assert!(prompt.ends_with("- 본문 텍스트는 Korean로 작성하라.\n"));
    }

    #[test]
    fn user_prompt_uses_language_label_for_general_input() {
        let input = PromptInput::from_general("  my answer  ");

        assert_eq!(
            "User answer:\nmy answer",
            build_user_prompt(&input, &[], "en")
        );
        assert_eq!(
            "사용자 답변:\nmy answer",
            build_user_prompt(&input, &[], "ko")
        );
    }

    #[test]
    fn turn_query_joins_question_and_answer() {
        assert_eq!("q\na", build_turn_query(" q ", " a "));
        assert_eq!("a", build_turn_query("  ", "a"));
        assert_eq!("q", build_turn_query("q", ""));
    }
}
