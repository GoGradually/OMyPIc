package me.go_gradually.omypic.domain.session;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

class TurnBatchingPolicyTest {

    @Test
    void onAnsweredTurn_immediateMode_emitsEveryTurn() {
        TurnBatchingPolicy.BatchDecision decision = TurnBatchingPolicy.onAnsweredTurn(ModeType.IMMEDIATE, 5, 3);

        assertTrue(decision.emitFeedback());
        assertEquals(0, decision.nextAnsweredCount());
        assertEquals(TurnBatchingPolicy.BatchReason.IMMEDIATE_MODE, decision.reason());
    }

    @Test
    void onAnsweredTurn_continuousMode_emitsOnBatchBoundary() {
        TurnBatchingPolicy.BatchDecision waiting = TurnBatchingPolicy.onAnsweredTurn(ModeType.CONTINUOUS, 0, 2);
        TurnBatchingPolicy.BatchDecision ready = TurnBatchingPolicy.onAnsweredTurn(ModeType.CONTINUOUS, 1, 2);

        assertFalse(waiting.emitFeedback());
        assertEquals(1, waiting.nextAnsweredCount());
        assertEquals(TurnBatchingPolicy.BatchReason.WAITING_FOR_BATCH, waiting.reason());

        assertTrue(ready.emitFeedback());
        assertEquals(0, ready.nextAnsweredCount());
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
