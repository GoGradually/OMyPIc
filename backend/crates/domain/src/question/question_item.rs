use super::QuestionItemId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestionItem {
    id: QuestionItemId,
    text: String,
    question_type: Option<String>,
}

impl QuestionItem {
    pub fn rehydrate(id: QuestionItemId, text: &str, question_type: Option<&str>) -> Self {
        Self {
            id,
            text: text.to_string(),
            question_type: normalize_question_type(question_type),
        }
    }

    pub fn create(text: &str, question_type: Option<&str>) -> Self {
        Self::rehydrate(QuestionItemId::new_id(), text, question_type)
    }

    pub fn with_text_and_question_type(&self, text: &str, question_type: Option<&str>) -> Self {
        Self::rehydrate(self.id.clone(), text, question_type)
    }

    pub fn id(&self) -> &QuestionItemId {
        &self.id
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn question_type(&self) -> Option<&str> {
        self.question_type.as_deref()
    }
}

fn normalize_question_type(question_type: Option<&str>) -> Option<String> {
    let normalized = question_type?.trim();
    if normalized.is_empty() {
        None
    } else {
        Some(normalized.to_string())
    }
}
