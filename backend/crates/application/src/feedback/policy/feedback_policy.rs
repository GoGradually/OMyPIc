pub trait FeedbackPolicy: Send + Sync {
    fn get_summary_max_chars(&self) -> usize;
    fn get_example_min_ratio(&self) -> f64;
    fn get_example_max_ratio(&self) -> f64;
    fn get_wrongnote_summary_max_chars(&self) -> usize;
    fn get_wrongnote_window_size(&self) -> usize {
        100
    }
}
