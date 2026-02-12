package me.go_gradually.omypic.domain.session;

import me.go_gradually.omypic.domain.question.QuestionGroup;
import me.go_gradually.omypic.domain.question.QuestionItem;
import me.go_gradually.omypic.domain.question.QuestionItemId;
import me.go_gradually.omypic.domain.question.QuestionList;
import me.go_gradually.omypic.domain.question.QuestionListId;
import org.junit.jupiter.api.Test;

import java.time.Instant;
import java.util.List;

import static org.junit.jupiter.api.Assertions.*;

class SessionStateTest {

    @Test
    void shouldGenerateFeedback_inContinuousMode_onlyOnBatchBoundary() {
        SessionState state = new SessionState(SessionId.of("session-1"));
        state.applyModeUpdate(ModeType.CONTINUOUS, 3);

        assertFalse(state.shouldGenerateFeedback());
        assertEquals(1, state.getAnsweredSinceLastFeedback());

        assertFalse(state.shouldGenerateFeedback());
        assertEquals(2, state.getAnsweredSinceLastFeedback());

        assertTrue(state.shouldGenerateFeedback());
        assertEquals(0, state.getAnsweredSinceLastFeedback());
    }

    @Test
    void resolveFeedbackInputText_inContinuousMode_usesMostRecentNAnswers() {
        SessionState state = new SessionState(SessionId.of("session-batch"));
        state.applyModeUpdate(ModeType.CONTINUOUS, 2);
        state.appendSegment("one");
        state.appendSegment("two");
        state.appendSegment("three");

        String input = state.resolveFeedbackInputText("fallback");

        assertEquals("two\nthree", input);
    }

    @Test
    void applyModeUpdate_clampsBatchSizeAndKeepsModeWhenNull() {
        SessionState state = new SessionState(SessionId.of("session-2"));

        state.applyModeUpdate(ModeType.CONTINUOUS, 100);
        assertEquals(10, state.getContinuousBatchSize());

        state.applyModeUpdate(null, 0);
        assertEquals(ModeType.CONTINUOUS, state.getMode());
        assertEquals(1, state.getContinuousBatchSize());
    }

    @Test
    void nextQuestion_inSequentialMode_stopsWhenQuestionsAreExhausted() {
        QuestionList list = QuestionList.rehydrate(
                QuestionListId.of("seq-1"),
                "seq-list",
                List.of(
                        QuestionItem.rehydrate(QuestionItemId.of("q1"), "Q1", QuestionGroup.of("A")),
                        QuestionItem.rehydrate(QuestionItemId.of("q2"), "Q2", QuestionGroup.of("B"))
                ),
                Instant.parse("2026-01-01T00:00:00Z"),
                Instant.parse("2026-01-01T00:00:00Z")
        );
        SessionState state = new SessionState(SessionId.of("seq-session"));

        assertTrue(state.nextQuestion(list).isPresent());
        assertTrue(state.nextQuestion(list).isPresent());
        assertTrue(state.nextQuestion(list).isEmpty());

        state.resetQuestionProgress("seq-1");
        assertTrue(state.nextQuestion(list).isPresent());
    }
}
