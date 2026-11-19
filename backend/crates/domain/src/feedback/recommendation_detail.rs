use crate::shared::text_utils::java_trim;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecommendationDetail {
    term: String,
    usage: String,
}

impl RecommendationDetail {
    pub fn new(term: &str, usage: &str) -> Self {
        Self {
            term: java_trim(term).to_string(),
            usage: java_trim(usage).to_string(),
        }
    }

    pub fn term(&self) -> &str {
        &self.term
    }

    pub fn usage(&self) -> &str {
        &self.usage
    }
}
