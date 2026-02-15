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

    @Test
    void conversationState_updatesAndResets() {
        SessionState state = new SessionState(SessionId.of("session-4"));
        state.updateConversationState(new LlmConversationState("conv-1", "resp-1", 2));
        state.markLlmBootstrapped();

        assertEquals("conv-1", state.conversationState().conversationId());
        assertEquals("resp-1", state.conversationState().responseId());
        assertTrue(state.shouldRebaseConversation(2));
        assertTrue(state.isLlmBootstrapped());

        state.resetConversationState();
        assertEquals("", state.conversationState().conversationId());
        assertEquals(0, state.conversationState().turnCountSinceRebase());
        assertFalse(state.isLlmBootstrapped());
    }

    @Test
    void resetConversationState_withoutClearingBootstrap_keepsFlag() {
        SessionState state = new SessionState(SessionId.of("session-4b"));
        state.markLlmBootstrapped();
        state.updateConversationState(new LlmConversationState("conv-1", "resp-1", 2));

        state.resetConversationState(false);

        assertTrue(state.isLlmBootstrapped());
        assertEquals("", state.conversationState().conversationId());
    }

    @Test
    void appendLlmTurn_keepsRecentWindow() {
        SessionState state = new SessionState(SessionId.of("session-5"));
        state.appendLlmTurn("q1", "a1", "s1", 2);
        state.appendLlmTurn("q2", "a2", "s2", 2);
        state.appendLlmTurn("q3", "a3", "s3", 2);

        assertEquals(2, state.buildPromptContext().recentTurns().size());
        assertEquals("q2", state.buildPromptContext().recentTurns().get(0).question());
        assertEquals("q3", state.buildPromptContext().recentTurns().get(1).question());
    }
}
