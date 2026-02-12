package me.go_gradually.omypic.domain.session;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.*;

class TurnBatchingPolicyTest {

    @Test
    void onAnsweredTurn_inImmediateMode_emitsImmediately() {
        TurnBatchingPolicy.BatchDecision decision = TurnBatchingPolicy.onAnsweredTurn(ModeType.IMMEDIATE, 0, 3);

        assertTrue(decision.emitFeedback());
        assertEquals(0, decision.nextAnsweredCount());
        assertEquals(TurnBatchingPolicy.BatchReason.IMMEDIATE_MODE, decision.reason());
    }

    @Test
    void onAnsweredTurn_inContinuousMode_waitsUntilBatchBoundary() {
        TurnBatchingPolicy.BatchDecision waiting = TurnBatchingPolicy.onAnsweredTurn(ModeType.CONTINUOUS, 1, 3);

        assertFalse(waiting.emitFeedback());
        assertEquals(2, waiting.nextAnsweredCount());
        assertEquals(TurnBatchingPolicy.BatchReason.WAITING_FOR_BATCH, waiting.reason());

        TurnBatchingPolicy.BatchDecision emit = TurnBatchingPolicy.onAnsweredTurn(ModeType.CONTINUOUS, 2, 3);
        assertTrue(emit.emitFeedback());
        assertEquals(0, emit.nextAnsweredCount());
        assertEquals(TurnBatchingPolicy.BatchReason.BATCH_READY, emit.reason());
    }

    @Test
    void shouldEmitResidualContinuousBatch_returnsTrueOnlyForContinuousExhaustedWithoutEmission() {
        assertTrue(TurnBatchingPolicy.shouldEmitResidualContinuousBatch(ModeType.CONTINUOUS, true, false));
        assertFalse(TurnBatchingPolicy.shouldEmitResidualContinuousBatch(ModeType.CONTINUOUS, true, true));
        assertFalse(TurnBatchingPolicy.shouldEmitResidualContinuousBatch(ModeType.IMMEDIATE, true, false));
    }
}
