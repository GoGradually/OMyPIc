use crate::shared::text_utils::{java_is_blank, java_trim};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmConversationState {
    conversation_id: String,
    response_id: String,
    turn_count_since_rebase: i32,
}

impl LlmConversationState {
    pub fn new(conversation_id: &str, response_id: &str, turn_count_since_rebase: i32) -> Self {
        Self {
            conversation_id: java_trim(conversation_id).to_string(),
            response_id: java_trim(response_id).to_string(),
            turn_count_since_rebase: turn_count_since_rebase.max(0),
        }
    }

    pub fn empty() -> Self {
        Self::new("", "", 0)
    }

    pub fn has_conversation_id(&self) -> bool {
        !java_is_blank(&self.conversation_id)
    }

    pub fn conversation_id(&self) -> &str {
        &self.conversation_id
    }

    pub fn response_id(&self) -> &str {
        &self.response_id
    }

    pub fn turn_count_since_rebase(&self) -> i32 {
        self.turn_count_since_rebase
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_normalizes_values() {
        let state = LlmConversationState::new("  conv ", " resp", -3);

        assert_eq!("conv", state.conversation_id());
        assert_eq!("resp", state.response_id());
        assert_eq!(0, state.turn_count_since_rebase());
        assert!(state.has_conversation_id());
        assert!(!LlmConversationState::empty().has_conversation_id());
    }
}
