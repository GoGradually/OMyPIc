use super::SessionStopReason;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NextActionType {
    AskNext,
    AutoStop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionAction {
    action_type: NextActionType,
    reason: Option<SessionStopReason>,
}

impl SessionAction {
    pub fn ask_next() -> Self {
        Self {
            action_type: NextActionType::AskNext,
            reason: None,
        }
    }

    /// Java는 reason이 null이면 예외를 던지지만, Rust에서는 타입이 null을 막는다.
    pub fn auto_stop(reason: SessionStopReason) -> Self {
        Self {
            action_type: NextActionType::AutoStop,
            reason: Some(reason),
        }
    }

    pub fn action_type(&self) -> NextActionType {
        self.action_type
    }

    pub fn reason(&self) -> Option<SessionStopReason> {
        self.reason
    }
}

pub struct SessionFlowPolicy;

impl SessionFlowPolicy {
    pub fn decide_after_question_selection(exhausted: bool) -> SessionAction {
        if exhausted {
            return SessionAction::auto_stop(SessionStopReason::QuestionExhausted);
        }
        SessionAction::ask_next()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decide_after_question_selection_returns_ask_next_when_not_exhausted() {
        let action = SessionFlowPolicy::decide_after_question_selection(false);

        assert_eq!(NextActionType::AskNext, action.action_type());
        assert_eq!(None, action.reason());
    }

    #[test]
    fn decide_after_question_selection_returns_auto_stop_when_exhausted() {
        let action = SessionFlowPolicy::decide_after_question_selection(true);

        assert_eq!(NextActionType::AutoStop, action.action_type());
        assert_eq!(Some(SessionStopReason::QuestionExhausted), action.reason());
        assert_eq!(
            "QUESTION_EXHAUSTED",
            SessionStopReason::QuestionExhausted.code()
        );
    }
}
