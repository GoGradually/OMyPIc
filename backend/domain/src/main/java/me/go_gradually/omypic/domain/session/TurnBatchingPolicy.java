package me.go_gradually.omypic.domain.session;

/**
 * 피드백 주기를 결정하기 위한 도메인 로직
 */
public final class TurnBatchingPolicy {
    private TurnBatchingPolicy() {
    }

    public static BatchDecision onAnsweredTurn(ModeType mode, int answeredSinceLastFeedback, int continuousBatchSize) {
        ModeType resolvedMode = mode == null ? ModeType.IMMEDIATE : mode;
        int resolvedBatchSize = Math.max(1, continuousBatchSize);
        int currentCount = Math.max(0, answeredSinceLastFeedback);

        if (resolvedMode == ModeType.MOCK_EXAM) {
            return new BatchDecision(false, currentCount, BatchReason.WAITING_FOR_MOCK_EXAM_COMPLETION);
        }
        if (resolvedMode == ModeType.IMMEDIATE) {
            return new BatchDecision(true, 0, BatchReason.IMMEDIATE_MODE);
        }

        int nextCount = currentCount + 1;
        if (nextCount < resolvedBatchSize) {
            return new BatchDecision(false, nextCount, BatchReason.WAITING_FOR_BATCH);
        }
        return new BatchDecision(true, 0, BatchReason.BATCH_READY);
    }

    public static boolean shouldEmitResidualContinuousBatch(ModeType mode,
                                                            boolean questionExhausted,
                                                            boolean emittedFeedbackThisTurn) {
        return mode == ModeType.CONTINUOUS && questionExhausted && !emittedFeedbackThisTurn;
    }

    public enum BatchReason {
        IMMEDIATE_MODE,
        WAITING_FOR_BATCH,
        BATCH_READY,
        MOCK_EXAM_FINAL,
        WAITING_FOR_MOCK_EXAM_COMPLETION,
        EXHAUSTED_WITH_REMAINDER
    }

    public record BatchDecision(boolean emitFeedback, int nextAnsweredCount, BatchReason reason) {
    }
}
