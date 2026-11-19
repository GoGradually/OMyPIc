use crate::shared::text_utils::java_trim;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CorrectionDetail {
    issue: String,
    fix: String,
}

impl CorrectionDetail {
    pub fn new(issue: &str, fix: &str) -> Self {
        Self {
            issue: java_trim(issue).to_string(),
            fix: java_trim(fix).to_string(),
        }
    }

    pub fn issue(&self) -> &str {
        &self.issue
    }

    pub fn fix(&self) -> &str {
        &self.fix
    }
}
