/// 문자(char) 수 기준으로 `max_chars - 1`까지 자르고 양끝 공백을 제거한다.
pub fn trim_to_length(text: Option<&str>, max_chars: usize) -> String {
    let Some(text) = text else {
        return String::new();
    };
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let keep = max_chars.saturating_sub(1);
    text.chars()
        .take(keep)
        .collect::<String>()
        .trim()
        .to_string()
}

/// 문자(char) 수 기준으로 고정 크기 조각으로 나눈다. 공백뿐이거나 없으면 빈 목록.
pub fn split_chunks(text: Option<&str>, max_chunk_size: usize) -> Vec<String> {
    let Some(text) = text else {
        return Vec::new();
    };
    if text.trim().is_empty() {
        return Vec::new();
    }
    let chars: Vec<char> = text.chars().collect();
    chars
        .chunks(max_chunk_size.max(1))
        .map(|chunk| chunk.iter().collect())
        .collect()
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
}
