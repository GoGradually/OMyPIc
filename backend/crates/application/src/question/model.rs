#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NextQuestion {
    pub question_id: Option<String>,
    pub text: Option<String>,
    pub group_id: Option<String>,
    pub group: Option<String>,
    pub question_type: Option<String>,
    pub skipped: bool,
}

impl NextQuestion {
    pub fn skipped() -> Self {
        Self {
            skipped: true,
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestionTagStat {
    pub tag: String,
    pub group_count: u64,
    pub selectable: bool,
}
