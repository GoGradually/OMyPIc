//! Java `String`과 같은 길이 규칙을 따르는 문자열 도구.
//!
//! Java의 `String.length()`와 `substring`은 UTF-16 코드 유닛 기준이다. 여기서도 같은
//! 기준으로 센다. 다만 Rust `String`은 홀로 남은 서로게이트를 담을 수 없으므로,
//! 자르는 지점이 서로게이트 쌍(이모지 등)의 한가운데면 그 문자는 통째로 뺀다.
//! (Java는 깨진 반쪽 문자를 남긴다.)

/// Java `String.length()`와 같은 UTF-16 코드 유닛 수.
pub fn utf16_len(text: &str) -> usize {
    text.chars().map(char::len_utf16).sum()
}

/// UTF-16 유닛 `max_units`개를 넘지 않는 가장 긴 앞부분. Java `substring(0, n)`에 대응하되
/// 서로게이트 쌍을 쪼개지 않는다.
pub fn utf16_prefix(text: &str, max_units: usize) -> &str {
    let mut units = 0;
    for (index, c) in text.char_indices() {
        units += c.len_utf16();
        if units > max_units {
            return &text[..index];
        }
    }
    text
}

/// Java `Character.isWhitespace`.
pub fn is_java_whitespace(c: char) -> bool {
    match c {
        '\u{00A0}' | '\u{2007}' | '\u{202F}' => false,
        '\t' | '\n' | '\u{000B}' | '\u{000C}' | '\r' | '\u{001C}'..='\u{001F}' => true,
        '\u{2028}' | '\u{2029}' => true,
        _ => matches!(
            c,
            ' ' | '\u{1680}' | '\u{2000}'..='\u{200A}' | '\u{205F}' | '\u{3000}'
        ),
    }
}

/// Java `String.trim()`: `U+0020` 이하의 문자만 양끝에서 제거한다.
pub fn java_trim(text: &str) -> &str {
    text.trim_matches(|c: char| c <= ' ')
}

/// Java `String.isBlank()`.
pub fn java_is_blank(text: &str) -> bool {
    text.chars().all(is_java_whitespace)
}

/// UTF-16 길이가 `max_chars`를 넘으면 `max_chars - 1`까지 자르고 양끝 공백을 제거한다.
pub fn trim_to_length(text: Option<&str>, max_chars: usize) -> String {
    let Some(text) = text else {
        return String::new();
    };
    if utf16_len(text) <= max_chars {
        return text.to_string();
    }
    java_trim(utf16_prefix(text, max_chars.saturating_sub(1))).to_string()
}

/// UTF-16 유닛 수 기준으로 고정 크기 조각으로 나눈다. 공백뿐이거나 없으면 빈 목록.
/// 서로게이트 쌍은 쪼개지 않으므로 쌍이 경계에 걸리면 그 조각이 한 유닛 짧아진다.
pub fn split_chunks(text: Option<&str>, max_chunk_size: usize) -> Vec<String> {
    let Some(text) = text else {
        return Vec::new();
    };
    if java_is_blank(text) {
        return Vec::new();
    }
    let max = max_chunk_size.max(1);
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut units = 0;
    for c in text.chars() {
        let width = c.len_utf16();
        if units > 0 && units + width > max {
            chunks.push(std::mem::take(&mut current));
            units = 0;
        }
        current.push(c);
        units += width;
        if units >= max {
            chunks.push(std::mem::take(&mut current));
            units = 0;
        }
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trim_to_length_returns_original_when_short_enough() {
        assert_eq!("abc", trim_to_length(Some("abc"), 5));
    }

    #[test]
    fn trim_to_length_truncates_at_max_minus_one() {
        assert_eq!("abcd", trim_to_length(Some("abcdef"), 5));
    }

    #[test]
    fn trim_to_length_returns_empty_for_null() {
        assert_eq!("", trim_to_length(None, 5));
    }

    #[test]
    fn split_chunks_returns_empty_for_blank_text() {
        assert!(split_chunks(Some("   "), 3).is_empty());
    }

    #[test]
    fn split_chunks_splits_text_into_fixed_size_chunks() {
        let chunks = split_chunks(Some("abcdefghij"), 4);

        assert_eq!(vec!["abcd", "efgh", "ij"], chunks);
    }

    // "한글😀abc"는 Java에서 length() == 7이다 (한 1, 글 1, 😀 2, a b c 3).
    const MIXED: &str = "한글😀abc";

    #[test]
    fn utf16_len_counts_surrogate_pairs_as_two() {
        assert_eq!(7, utf16_len(MIXED));
        assert_eq!(6, MIXED.chars().count());
        assert_eq!(2, utf16_len("😀"));
        assert_eq!(3, utf16_len("한글한"));
    }

    #[test]
    fn trim_to_length_counts_utf16_units_like_java() {
        // Java: 7 <= 7 이므로 그대로.
        assert_eq!(MIXED, trim_to_length(Some(MIXED), 7));
        // Java: 7 > 6 이므로 substring(0, 5) == "한글😀a". char 기준이면 잘리지 않았을 입력이다.
        assert_eq!("한글😀a", trim_to_length(Some(MIXED), 6));
        // Java: substring(0, 4) == "한글😀"
        assert_eq!("한글😀", trim_to_length(Some(MIXED), 5));
    }

    #[test]
    fn trim_to_length_drops_a_half_surrogate_pair() {
        // Java: substring(0, 3)은 "한글" + 깨진 high surrogate. Rust는 쌍을 통째로 뺀다.
        assert_eq!("한글", trim_to_length(Some(MIXED), 4));
    }

    #[test]
    fn trim_to_length_trims_whitespace_after_cutting() {
        assert_eq!("한글", trim_to_length(Some("한글 😀abc"), 4));
    }

    #[test]
    fn java_trim_only_strips_control_and_space_characters() {
        assert_eq!("a", java_trim(" \t\na\r "));
        assert_eq!("\u{3000}a\u{3000}", java_trim("\u{3000}a\u{3000}"));
        assert_eq!("\u{00A0}a", java_trim("\u{00A0}a"));
    }

    #[test]
    fn java_is_blank_follows_character_is_whitespace() {
        assert!(java_is_blank(""));
        assert!(java_is_blank(" \t\u{3000}"));
        assert!(!java_is_blank("\u{00A0}"));
        assert!(!java_is_blank(" a "));
    }

    #[test]
    fn split_chunks_counts_utf16_units_like_java() {
        // Java: "한글한글한"을 4유닛씩 -> "한글한글", "한"
        assert_eq!(vec!["한글한글", "한"], split_chunks(Some("한글한글한"), 4));
        // Java: ["한글" + 깨진 쌍 앞, 쌍 뒤 + "ab", "c"]. Rust는 쌍을 쪼개지 않는다.
        let chunks = split_chunks(Some(MIXED), 3);
        assert_eq!(vec!["한글", "😀a", "bc"], chunks);
        assert_eq!(MIXED, chunks.concat());
    }

    #[test]
    fn split_chunks_keeps_a_surrogate_pair_whole_when_chunk_size_is_one() {
        assert_eq!(vec!["a", "😀", "b"], split_chunks(Some("a😀b"), 1));
    }
}
