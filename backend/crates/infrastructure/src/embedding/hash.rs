//! 해시 임베딩: 토큰을 버킷에 세어 넣고 L2 정규화한다.

use omypic_domain::error::AppResult;

use super::{is_blank, normalize, split_regex_space};
use omypic_application::rulebook::port::EmbeddingPort;

#[derive(Debug, Clone)]
pub struct LocalHashEmbeddingAdapter {
    dimension: usize,
}

impl LocalHashEmbeddingAdapter {
    pub fn new(dimension: usize) -> Self {
        Self { dimension }
    }

    /// 포트의 `Result`를 거치지 않는 계산. fastText 어댑터의 대체 경로가 쓴다.
    pub(crate) fn embed_text(&self, text: &str) -> Vec<f32> {
        let mut vector = vec![0.0_f32; self.dimension];
        if self.dimension == 0 || is_blank(text) {
            return vector;
        }
        for token in split_regex_space(&text.to_lowercase()).filter(|t| !is_blank(t)) {
            let bucket = floor_mod(hash(token), self.dimension);
            vector[bucket] += 1.0;
        }
        normalize(&mut vector);
        vector
    }
}

/// Java `Math.floorMod(int, int)`.
fn floor_mod(value: i32, dimension: usize) -> usize {
    i64::from(value).rem_euclid(dimension as i64) as usize
}

/// UTF-8 바이트(부호 있는 `byte`)로 `h = 31 * h + b`를 32비트 정수로 계산한다.
fn hash(token: &str) -> i32 {
    let mut h: i32 = 0;
    for byte in token.bytes() {
        h = h.wrapping_mul(31).wrapping_add(i32::from(byte as i8));
    }
    h
}

impl EmbeddingPort for LocalHashEmbeddingAdapter {
    fn embed(&self, text: &str) -> AppResult<Vec<f32>> {
        Ok(self.embed_text(text))
    }

    fn dimension(&self) -> usize {
        self.dimension
    }

    fn provider(&self) -> String {
        "hash".to_string()
    }

    fn model_version(&self) -> String {
        "local-hash-v1".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn l2_norm(vector: &[f32]) -> f64 {
        vector
            .iter()
            .map(|v| f64::from(*v) * f64::from(*v))
            .sum::<f64>()
            .sqrt()
    }

    #[test]
    fn embed_returns_zero_vector_for_blank_text() {
        let adapter = LocalHashEmbeddingAdapter::new(8);
        let blank = adapter.embed("   ").unwrap();
        let empty = adapter.embed("").unwrap();
        assert_eq!(8, blank.len());
        assert_eq!(8, empty.len());
        assert!(l2_norm(&blank).abs() < 1e-9);
        assert!(l2_norm(&empty).abs() < 1e-9);
    }

    #[test]
    fn embed_normalizes_non_empty_vector() {
        let adapter = LocalHashEmbeddingAdapter::new(8);
        let vector = adapter.embed("Hello hello world").unwrap();
        assert_eq!(8, vector.len());
        assert!((l2_norm(&vector) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn dimension_returns_configured_dimension() {
        assert_eq!(16, LocalHashEmbeddingAdapter::new(16).dimension());
        assert_eq!("hash", LocalHashEmbeddingAdapter::new(16).provider());
        assert_eq!(
            "local-hash-v1",
            LocalHashEmbeddingAdapter::new(16).model_version()
        );
    }

    // 값은 Java 알고리즘(UTF-8 바이트로 h = 31 * h + b, floorMod, 누적 후 정규화)을
    // 손으로 계산한 것이다.
    //   "안녕"   -> h = -674998788, floorMod(h, 16) = 12
    //   "하세요" -> h = -315585066, floorMod(h, 16) = 6
    //   벡터 [.. 6: 1, 12: 2 ..] / sqrt(5)
    #[test]
    fn korean_vector_matches_java_values() {
        let vector = LocalHashEmbeddingAdapter::new(16)
            .embed("안녕 하세요 안녕")
            .unwrap();
        let mut expected = vec![0.0_f32; 16];
        expected[6] = 0.447_213_6_f32;
        expected[12] = 0.894_427_2_f32;
        assert_eq!(expected, vector);
    }

    //   "hello" -> h = 99162322, mod 7 = 0 (두 번)
    //   "world" -> h = 113318802, mod 7 = 2
    #[test]
    fn english_vector_matches_java_values() {
        let vector = LocalHashEmbeddingAdapter::new(7)
            .embed("Hello hello world")
            .unwrap();
        let mut expected = vec![0.0_f32; 7];
        expected[0] = 0.894_427_2_f32;
        expected[2] = 0.447_213_6_f32;
        assert_eq!(expected, vector);
    }

    #[test]
    fn tokens_of_only_java_whitespace_are_skipped() {
        let adapter = LocalHashEmbeddingAdapter::new(16);
        let expected = adapter.embed("a b").unwrap();
        assert_eq!(expected, adapter.embed("a \u{3000} b").unwrap());
        assert_eq!(expected, adapter.embed("a \u{1C} b").unwrap());
    }

    #[test]
    fn hash_wraps_like_java_int_and_uses_signed_bytes() {
        assert_eq!(99_162_322, hash("hello"));
        assert_eq!(-674_998_788, hash("안녕"));
        assert_eq!(12, floor_mod(-674_998_788, 16));
    }
}
