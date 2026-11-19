use super::ModeType;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchReason {
    ImmediateMode,
    WaitingForGroupCompletion,
    WaitingForBatch,
    BatchReady,
    ExhaustedWithRemainder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatchDecision {
    emit_feedback: bool,
    next_completed_group_count: i32,
    reason: BatchReason,
}

impl BatchDecision {
    pub fn new(emit_feedback: bool, next_completed_group_count: i32, reason: BatchReason) -> Self {
        Self {
            emit_feedback,
            next_completed_group_count,
            reason,
        }
    }

    pub fn emit_feedback(&self) -> bool {
        self.emit_feedback
    }

    pub fn next_completed_group_count(&self) -> i32 {
        self.next_completed_group_count
    }

    pub fn reason(&self) -> BatchReason {
        self.reason
    }
}

/// 피드백 주기를 결정하기 위한 도메인 로직
pub struct TurnBatchingPolicy;

impl TurnBatchingPolicy {
    pub fn on_turn(
        mode: ModeType,
        completed_group_count_since_last_feedback: i32,
        group_batch_size: i32,
        completed_group_this_turn: bool,
    ) -> BatchDecision {
        let batch_size = group_batch_size.max(1);
        let current_count = completed_group_count_since_last_feedback.max(0);

        if mode == ModeType::Immediate {
            return BatchDecision::new(true, 0, BatchReason::ImmediateMode);
        }

        if !completed_group_this_turn {
            return BatchDecision::new(
                false,
                current_count,
                BatchReason::WaitingForGroupCompletion,
            );
        }

        let next_count = current_count + 1;
        if next_count < batch_size {
            return BatchDecision::new(false, next_count, BatchReason::WaitingForBatch);
        }
        BatchDecision::new(true, 0, BatchReason::BatchReady)
    }

    pub fn should_emit_residual_continuous_batch(
        mode: ModeType,
        question_exhausted: bool,
        emitted_feedback_this_turn: bool,
    ) -> bool {
        mode == ModeType::Continuous && question_exhausted && !emitted_feedback_this_turn
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn on_turn_immediate_mode_emits_every_turn() {
        let decision = TurnBatchingPolicy::on_turn(ModeType::Immediate, 5, 3, true);

        assert!(decision.emit_feedback());
        assert_eq!(0, decision.next_completed_group_count());
        assert_eq!(BatchReason::ImmediateMode, decision.reason());
    }

    #[test]
    fn on_turn_continuous_mode_waits_for_group_completion_then_batch_boundary() {
        let waiting_completion = TurnBatchingPolicy::on_turn(ModeType::Continuous, 0, 2, false);
        let waiting_batch = TurnBatchingPolicy::on_turn(ModeType::Continuous, 0, 2, true);
        let ready = TurnBatchingPolicy::on_turn(ModeType::Continuous, 1, 2, true);

        assert!(!waiting_completion.emit_feedback());
        assert_eq!(
            BatchReason::WaitingForGroupCompletion,
            waiting_completion.reason()
        );

        assert!(!waiting_batch.emit_feedback());
        assert_eq!(1, waiting_batch.next_completed_group_count());
        assert_eq!(BatchReason::WaitingForBatch, waiting_batch.reason());

        assert!(ready.emit_feedback());
        assert_eq!(0, ready.next_completed_group_count());
        assert_eq!(BatchReason::BatchReady, ready.reason());
    }

    #[test]
    fn should_emit_residual_continuous_batch_only_for_continuous_exhausted_without_emit() {
        assert!(TurnBatchingPolicy::should_emit_residual_continuous_batch(
            ModeType::Continuous,
            true,
            false
        ));
        assert!(!TurnBatchingPolicy::should_emit_residual_continuous_batch(
            ModeType::Continuous,
            true,
            true
        ));
        assert!(!TurnBatchingPolicy::should_emit_residual_continuous_batch(
            ModeType::Immediate,
            true,
            false
        ));
        assert!(!TurnBatchingPolicy::should_emit_residual_continuous_batch(
            ModeType::Continuous,
            false,
            false
        ));
    }
}
