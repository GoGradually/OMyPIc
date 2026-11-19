//! 임베딩 어댑터(fastText, 해시). Java `infrastructure.rulebook.rag`의 계산을 그대로 따른다.

mod fasttext;
mod hash;

pub use fasttext::FastTextEmbeddingAdapter;
pub use hash::LocalHashEmbeddingAdapter;

/// Java 정규식 `\s`(ASCII 공백 문자)에 해당하는지.
pub(crate) fn is_regex_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\u{0B}' | '\u{0C}' | '\r')
}

pub(crate) use omypic_domain::shared::text_utils::java_is_blank as is_blank;

/// Java `split("\\s+")`에서 빈 토큰을 뺀 것. 공백만 남은 토큰(`\s`가 나누지 않는 공백)은
/// 호출하는 쪽이 `is_blank`로 걸러야 한다.
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
}
