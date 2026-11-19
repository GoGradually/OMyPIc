#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionStopReason {
    QuestionExhausted,
}

impl SessionStopReason {
    pub fn code(&self) -> &'static str {
        match self {
            SessionStopReason::QuestionExhausted => "QUESTION_EXHAUSTED",
        }
    }
}
