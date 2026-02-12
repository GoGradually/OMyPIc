package me.go_gradually.omypic.domain.session;

import org.junit.jupiter.api.Test;

import java.util.List;
import java.util.Set;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

class SessionStateTest {

    @Test
    void decideFeedbackBatchOnTurn_inContinuousMode_usesCompletedGroupCount() {
        SessionState state = new SessionState(SessionId.of("session-1"));
        state.applyModeUpdate(ModeType.CONTINUOUS, 2);

        TurnBatchingPolicy.BatchDecision waitingForCompletion = state.decideFeedbackBatchOnTurn(false);
        assertFalse(waitingForCompletion.emitFeedback());
        assertEquals(0, state.getCompletedGroupCountSinceLastFeedback());

        TurnBatchingPolicy.BatchDecision waitingForBatch = state.decideFeedbackBatchOnTurn(true);
        assertFalse(waitingForBatch.emitFeedback());
        assertEquals(1, state.getCompletedGroupCountSinceLastFeedback());

        TurnBatchingPolicy.BatchDecision ready = state.decideFeedbackBatchOnTurn(true);
        assertTrue(ready.emitFeedback());
        assertEquals(0, state.getCompletedGroupCountSinceLastFeedback());
    }

    @Test
    void configureQuestionGroups_resetsProgressAndStoresTags() {
        SessionState state = new SessionState(SessionId.of("session-2"));
        state.configureQuestionGroups(Set.of("travel", "habit"), List.of("g1", "g2"));

        assertEquals(Set.of("travel", "habit"), state.getSelectedGroupTags());
        assertEquals(List.of("g1", "g2"), state.getCandidateGroupOrder());
        assertEquals("g1", state.currentCandidateGroupId());

        state.markQuestionAsked("g1");
        assertEquals(1, state.getCurrentQuestionIndex("g1"));

        state.moveToNextGroup();
        assertEquals("g2", state.currentCandidateGroupId());
    }

    @Test
    void shouldGenerateResidualContinuousFeedback_onlyForContinuousExhaustedWithoutEmit() {
        SessionState state = new SessionState(SessionId.of("session-3"));
        state.applyModeUpdate(ModeType.CONTINUOUS, 3);

        assertTrue(state.shouldGenerateResidualContinuousFeedback(true, false));
        assertFalse(state.shouldGenerateResidualContinuousFeedback(true, true));
        assertFalse(state.shouldGenerateResidualContinuousFeedback(false, false));
    }
}
