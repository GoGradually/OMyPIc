#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FeedbackConstraints {
    pub summary_max_chars: usize,
    pub example_min_ratio: f64,
    pub example_max_ratio: f64,
}

impl FeedbackConstraints {
    pub fn new(summary_max_chars: usize, example_min_ratio: f64, example_max_ratio: f64) -> Self {
        Self {
            summary_max_chars,
            example_min_ratio,
            example_max_ratio,
        }
    }
}
