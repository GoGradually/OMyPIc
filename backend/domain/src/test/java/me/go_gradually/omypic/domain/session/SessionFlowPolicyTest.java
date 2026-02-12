package me.go_gradually.omypic.domain.session;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNull;

class SessionFlowPolicyTest {

    @Test
    void decideAfterQuestionSelection_returnsAskNextWhenNotExhausted() {
        SessionFlowPolicy.SessionAction action = SessionFlowPolicy.decideAfterQuestionSelection(false);

        assertEquals(SessionFlowPolicy.NextActionType.ASK_NEXT, action.type());
        assertNull(action.reason());
    }

    @Test
    void decideAfterQuestionSelection_returnsAutoStopWhenExhausted() {
        SessionFlowPolicy.SessionAction action = SessionFlowPolicy.decideAfterQuestionSelection(true);

        assertEquals(SessionFlowPolicy.NextActionType.AUTO_STOP, action.type());
        assertEquals(SessionStopReason.QUESTION_EXHAUSTED, action.reason());
    }
}
