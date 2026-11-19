use super::RulebookId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RulebookContext {
    rulebook_id: RulebookId,
    filename: String,
    text: String,
}

impl RulebookContext {
    pub fn of(rulebook_id: RulebookId, filename: Option<&str>, text: Option<&str>) -> Self {
        Self {
            rulebook_id,
            filename: filename.unwrap_or_default().to_string(),
            text: text.unwrap_or_default().to_string(),
        }
    }

    pub fn rulebook_id(&self) -> &RulebookId {
        &self.rulebook_id
    }

    pub fn filename(&self) -> &str {
        &self.filename
    }

    pub fn text(&self) -> &str {
        &self.text
    }
}
