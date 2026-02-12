package me.go_gradually.omypic.domain.session;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

class TurnBatchingPolicyTest {

    @Test
    void onTurn_immediateMode_emitsEveryTurn() {
        TurnBatchingPolicy.BatchDecision decision = TurnBatchingPolicy.onTurn(ModeType.IMMEDIATE, 5, 3, true);

        assertTrue(decision.emitFeedback());
        assertEquals(0, decision.nextCompletedGroupCount());
        assertEquals(TurnBatchingPolicy.BatchReason.IMMEDIATE_MODE, decision.reason());
    }

    @Test
    void onTurn_continuousMode_waitsForGroupCompletionThenBatchBoundary() {
        TurnBatchingPolicy.BatchDecision waitingCompletion = TurnBatchingPolicy.onTurn(ModeType.CONTINUOUS, 0, 2, false);
        TurnBatchingPolicy.BatchDecision waitingBatch = TurnBatchingPolicy.onTurn(ModeType.CONTINUOUS, 0, 2, true);
        TurnBatchingPolicy.BatchDecision ready = TurnBatchingPolicy.onTurn(ModeType.CONTINUOUS, 1, 2, true);

        assertFalse(waitingCompletion.emitFeedback());
        assertEquals(TurnBatchingPolicy.BatchReason.WAITING_FOR_GROUP_COMPLETION, waitingCompletion.reason());

        assertFalse(waitingBatch.emitFeedback());
        assertEquals(1, waitingBatch.nextCompletedGroupCount());
        assertEquals(TurnBatchingPolicy.BatchReason.WAITING_FOR_BATCH, waitingBatch.reason());

        assertTrue(ready.emitFeedback());
        assertEquals(0, ready.nextCompletedGroupCount());
        assertEquals(TurnBatchingPolicy.BatchReason.BATCH_READY, ready.reason());
    }

    @Test
    void shouldEmitResidualContinuousBatch_onlyForContinuousExhaustedWithoutEmit() {
        assertTrue(TurnBatchingPolicy.shouldEmitResidualContinuousBatch(ModeType.CONTINUOUS, true, false));
        assertFalse(TurnBatchingPolicy.shouldEmitResidualContinuousBatch(ModeType.CONTINUOUS, true, true));
        assertFalse(TurnBatchingPolicy.shouldEmitResidualContinuousBatch(ModeType.IMMEDIATE, true, false));
        assertFalse(TurnBatchingPolicy.shouldEmitResidualContinuousBatch(ModeType.CONTINUOUS, false, false));
    }
}
