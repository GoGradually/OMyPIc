package me.go_gradually.omypic.domain.session;

import me.go_gradually.omypic.domain.question.QuestionItem;
import me.go_gradually.omypic.domain.question.QuestionItemId;
import me.go_gradually.omypic.domain.question.QuestionList;
import me.go_gradually.omypic.domain.question.QuestionListId;
import org.junit.jupiter.api.Test;

import java.time.Instant;
import java.util.*;

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
                        QuestionItem.rehydrate(QuestionItemId.of("q1"), "Q1", "A"),
                        QuestionItem.rehydrate(QuestionItemId.of("q2"), "Q2", "B")
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

    @Test
    void nextQuestion_inMockExamMode_usesConfiguredOrder_withoutDuplicates_andSkipsExhaustedGroups() {
        QuestionList list = QuestionList.rehydrate(
                QuestionListId.of("list-1"),
                "mock-list",
                List.of(
                        QuestionItem.rehydrate(QuestionItemId.of("a-1"), "A1", "A"),
                        QuestionItem.rehydrate(QuestionItemId.of("a-2"), "A2", "A"),
                        QuestionItem.rehydrate(QuestionItemId.of("b-1"), "B1", "B")
                ),
                Instant.parse("2026-01-01T00:00:00Z"),
                Instant.parse("2026-01-01T00:00:00Z")
        );

        SessionState state = new SessionState(SessionId.of("session-1"));
        state.applyModeUpdate(ModeType.MOCK_EXAM, null);
        state.configureMockExam(list, List.of("A", "B"), Map.of("A", 2, "B", 2));

        Optional<QuestionItem> first = state.nextQuestion(list);
        Optional<QuestionItem> second = state.nextQuestion(list);
        Optional<QuestionItem> third = state.nextQuestion(list);
        Optional<QuestionItem> fourth = state.nextQuestion(list);

        assertTrue(first.isPresent());
        assertTrue(second.isPresent());
        assertTrue(third.isPresent());
        assertTrue(fourth.isEmpty());

        assertEquals("A", first.orElseThrow().getGroup());
        assertEquals("A", second.orElseThrow().getGroup());
        assertEquals("B", third.orElseThrow().getGroup());
        assertTrue(state.isMockExamCompleted());

        Set<QuestionItemId> uniqueIds = new HashSet<>();
        uniqueIds.add(first.orElseThrow().getId());
        uniqueIds.add(second.orElseThrow().getId());
        uniqueIds.add(third.orElseThrow().getId());
        assertEquals(3, uniqueIds.size());
    }

    @Test
    void mockFinalFeedbackLifecycle_requiresCompletion_andAllowsSingleGeneration() {
        QuestionList list = QuestionList.rehydrate(
                QuestionListId.of("list-mock"),
                "mock",
                List.of(QuestionItem.rehydrate(QuestionItemId.of("q1"), "Q1", "A")),
                Instant.parse("2026-01-01T00:00:00Z"),
                Instant.parse("2026-01-01T00:00:00Z")
        );
        SessionState state = new SessionState(SessionId.of("session-mock"));
        state.applyModeUpdate(ModeType.MOCK_EXAM, null);
        state.configureMockExam(list, List.of("A"), Map.of("A", 1));
        state.appendSegment("answer one");

        assertThrows(IllegalStateException.class, state::buildMockFinalFeedbackInput);

        assertTrue(state.nextQuestion(list).isPresent());
        assertTrue(state.nextQuestion(list).isEmpty());
        assertTrue(state.isMockExamCompleted());

        assertEquals("answer one", state.buildMockFinalFeedbackInput());
        state.markMockFinalFeedbackGenerated();
        assertTrue(state.isMockFinalFeedbackGenerated());
        assertThrows(IllegalStateException.class, state::markMockFinalFeedbackGenerated);
    }

    @Test
    void buildMockFinalFeedbackInput_usesOnlyAnswersAfterMockExamConfigured() {
        QuestionList list = QuestionList.rehydrate(
                QuestionListId.of("list-mock-2"),
                "mock",
                List.of(QuestionItem.rehydrate(QuestionItemId.of("q1"), "Q1", "A")),
                Instant.parse("2026-01-01T00:00:00Z"),
                Instant.parse("2026-01-01T00:00:00Z")
        );
        SessionState state = new SessionState(SessionId.of("session-mock-2"));
        state.appendSegment("old-answer");
        state.applyModeUpdate(ModeType.MOCK_EXAM, null);
        state.configureMockExam(list, List.of("A"), Map.of("A", 1));
        state.appendSegment("mock-answer");
        state.nextQuestion(list);
        state.nextQuestion(list);

        assertEquals("mock-answer", state.buildMockFinalFeedbackInput());
    }

    @Test
    void nextQuestion_inMockExamWithoutConfig_fallsBackToSequential() {
        QuestionList list = QuestionList.rehydrate(
                QuestionListId.of("list-1"),
                "seq-list",
                List.of(
                        QuestionItem.rehydrate(QuestionItemId.of("q1"), "Q1", "A"),
                        QuestionItem.rehydrate(QuestionItemId.of("q2"), "Q2", "B")
                ),
                Instant.parse("2026-01-01T00:00:00Z"),
                Instant.parse("2026-01-01T00:00:00Z")
        );

        SessionState state = new SessionState(SessionId.of("session-3"));
        state.applyModeUpdate(ModeType.MOCK_EXAM, null);

        Optional<QuestionItem> first = state.nextQuestion(list);
        Optional<QuestionItem> second = state.nextQuestion(list);
        Optional<QuestionItem> third = state.nextQuestion(list);

        assertTrue(first.isPresent());
        assertTrue(second.isPresent());
        assertTrue(third.isEmpty());
        assertEquals("q1", first.orElseThrow().getId().value());
        assertEquals("q2", second.orElseThrow().getId().value());
    }
}
