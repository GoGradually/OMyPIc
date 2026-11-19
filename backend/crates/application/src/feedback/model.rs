use omypic_domain::feedback::Feedback;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FeedbackCommand {
    pub session_id: Option<String>,
    pub text: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub feedback_language: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FeedbackResult {
    pub generated: bool,
    pub feedback: Option<Feedback>,
}

impl FeedbackResult {
    pub fn generated(feedback: Feedback) -> Self {
        Self {
            generated: true,
            feedback: Some(feedback),
        }
    }

    pub fn skipped() -> Self {
        Self {
            generated: false,
            feedback: None,
        }
    }
}
