//! 임베딩 어댑터(fastText, 해시). Java `infrastructure.rulebook.rag`의 계산을 그대로 따른다.

mod fasttext;
mod hash;

pub use fasttext::FastTextEmbeddingAdapter;
pub use hash::LocalHashEmbeddingAdapter;

/// Java `Character.isWhitespace`.
pub(crate) fn is_java_whitespace(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{0B}' | '\u{0C}' | '\r' | '\u{1C}'..='\u{1F}' | ' '
    ) || (c != '\u{A0}' && c != '\u{2007}' && c != '\u{202F}' && is_unicode_separator(c))
}

fn is_unicode_separator(c: char) -> bool {
    matches!(
        c,
        '\u{1680}' | '\u{2000}'..='\u{200A}' | '\u{2028}' | '\u{2029}' | '\u{205F}' | '\u{3000}'
    )
}

/// Java 정규식 `\s`(ASCII 공백 문자)에 해당하는지.
pub(crate) fn is_regex_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\u{0B}' | '\u{0C}' | '\r')
}

/// Java `String.isBlank`.
pub(crate) fn is_blank(text: &str) -> bool {
    text.chars().all(is_java_whitespace)
}

/// Java `split("\\s+")`에서 빈 토큰을 뺀 것. 호출하는 쪽이 빈 토큰을 어차피 건너뛴다.
pub(crate) fn split_regex_space(text: &str) -> impl Iterator<Item = &str> {
    text.split(is_regex_space).filter(|part| !part.is_empty())
}

/// Java 코드의 `normalize`: `double` 누적, `(float)(v / norm)`.
pub(crate) fn normalize(vector: &mut [f32]) {
    let mut sum = 0.0_f64;
    for &v in vector.iter() {
        sum += f64::from(v * v);
    }
    if sum == 0.0 {
        return;
    }
    let norm = sum.sqrt();
    for v in vector.iter_mut() {
        *v = (f64::from(*v) / norm) as f32;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_skips_empty_tokens() {
        assert_eq!(
            vec!["a", "b"],
            split_regex_space(" a  b ").collect::<Vec<_>>()
        );
        assert_eq!(0, split_regex_space("   ").count());
    }

    #[test]
    fn java_whitespace_differs_from_unicode_white_space() {
        assert!(is_java_whitespace('\u{3000}'));
        assert!(is_java_whitespace('\u{1C}'));
        assert!(!is_java_whitespace('\u{A0}'));
        assert!(!is_java_whitespace('a'));
    }
}
