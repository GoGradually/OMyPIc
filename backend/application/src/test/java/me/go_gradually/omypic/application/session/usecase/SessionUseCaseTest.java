package me.go_gradually.omypic.application.session.usecase;

import me.go_gradually.omypic.application.question.port.QuestionListPort;
import me.go_gradually.omypic.application.session.model.ModeUpdateCommand;
import me.go_gradually.omypic.application.session.port.SessionStorePort;
import me.go_gradually.omypic.domain.question.QuestionGroup;
import me.go_gradually.omypic.domain.question.QuestionItem;
import me.go_gradually.omypic.domain.question.QuestionItemId;
import me.go_gradually.omypic.domain.question.QuestionList;
import me.go_gradually.omypic.domain.question.QuestionListId;
import me.go_gradually.omypic.domain.session.ModeType;
import me.go_gradually.omypic.domain.session.SessionId;
import me.go_gradually.omypic.domain.session.SessionState;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.extension.ExtendWith;
import org.mockito.Mock;
import org.mockito.junit.jupiter.MockitoExtension;

import java.time.Instant;
import java.util.List;
import java.util.Map;
import java.util.Optional;

import static org.junit.jupiter.api.Assertions.*;
import static org.mockito.Mockito.*;

@ExtendWith(MockitoExtension.class)
class SessionUseCaseTest {

    @Mock
    private SessionStorePort sessionStore;
    @Mock
    private QuestionListPort questionListPort;

    private SessionUseCase useCase;

    private static ModeUpdateCommand command(
            String sessionId,
            String listId,
            ModeType mode,
            Integer batch,
            List<String> order,
            Map<String, Integer> counts
    ) {
        ModeUpdateCommand command = new ModeUpdateCommand();
        command.setSessionId(sessionId);
        command.setListId(listId);
        command.setMode(mode);
        command.setContinuousBatchSize(batch);
        if (order != null || counts != null) {
            ModeUpdateCommand.MockPlan plan = new ModeUpdateCommand.MockPlan();
            plan.setGroupOrder(order);
            plan.setGroupCounts(counts);
            command.setMockPlan(plan);
        }
        return command;
    }

    @BeforeEach
    void setUp() {
        useCase = new SessionUseCase(sessionStore, questionListPort);
    }

    @Test
    void updateMode_clampsContinuousBatchSizeBetweenOneAndTen() {
        SessionState state = new SessionState(SessionId.of("s1"));
        when(sessionStore.getOrCreate(SessionId.of("s1"))).thenReturn(state);

        ModeUpdateCommand upper = command("s1", null, ModeType.CONTINUOUS, 100, null, null);
        useCase.updateMode(upper);
        assertEquals(10, state.getContinuousBatchSize());

        ModeUpdateCommand lower = command("s1", null, ModeType.CONTINUOUS, 0, null, null);
        useCase.updateMode(lower);
        assertEquals(1, state.getContinuousBatchSize());
    }

    @Test
    void updateMode_configuresMockExam_whenListIdProvided() {
        SessionState state = new SessionState(SessionId.of("s2"));
        QuestionList list = QuestionList.rehydrate(
                QuestionListId.of("list-1"),
                "list",
                List.of(
                        QuestionItem.rehydrate(QuestionItemId.of("q1"), "Q1", QuestionGroup.of("A")),
                        QuestionItem.rehydrate(QuestionItemId.of("q2"), "Q2", QuestionGroup.of("B"))
                ),
                Instant.parse("2026-01-01T00:00:00Z"),
                Instant.parse("2026-01-01T00:00:00Z")
        );

        when(sessionStore.getOrCreate(SessionId.of("s2"))).thenReturn(state);
        when(questionListPort.findById(QuestionListId.of("list-1"))).thenReturn(Optional.of(list));

        ModeUpdateCommand command = command(
                "s2",
                "list-1",
                ModeType.MOCK_EXAM,
                null,
                List.of("A", "B"),
                Map.of("A", 1, "B", 1)
        );

        SessionState updated = useCase.updateMode(command);

        assertEquals(ModeType.MOCK_EXAM, updated.getMode());
        assertNotNull(updated.getMockExamState());
        assertEquals(List.of(QuestionGroup.of("A"), QuestionGroup.of("B")), updated.getMockExamState().getGroupOrder());
    }

    @Test
    void updateMode_doesNotLookupList_whenListIdIsNull() {
        SessionState state = new SessionState(SessionId.of("s3"));
        when(sessionStore.getOrCreate(SessionId.of("s3"))).thenReturn(state);

        ModeUpdateCommand command = command("s3", null, ModeType.IMMEDIATE, null, null, null);
        useCase.updateMode(command);

        verify(questionListPort, never()).findById(QuestionListId.of("list-1"));
    }

    @Test
    void appendSegment_appendsTextToSession() {
        SessionState state = new SessionState(SessionId.of("s4"));
        when(sessionStore.getOrCreate(SessionId.of("s4"))).thenReturn(state);

        useCase.appendSegment("s4", "hello");

        assertEquals(1, state.getSttSegments().size());
        assertEquals("hello", state.getSttSegments().get(0));
    }

    @Test
    void updateMode_withMockModeAndMissingOrder_usesEmptyDefaults() {
        SessionState state = new SessionState(SessionId.of("s5"));
        QuestionList list = QuestionList.rehydrate(
                QuestionListId.of("list-2"),
                "list",
                List.of(QuestionItem.rehydrate(QuestionItemId.of("q1"), "Q1", QuestionGroup.of("A"))),
                Instant.parse("2026-01-01T00:00:00Z"),
                Instant.parse("2026-01-01T00:00:00Z")
        );
        when(sessionStore.getOrCreate(SessionId.of("s5"))).thenReturn(state);
        when(questionListPort.findById(QuestionListId.of("list-2"))).thenReturn(Optional.of(list));

        ModeUpdateCommand command = command("s5", "list-2", ModeType.MOCK_EXAM, null, null, null);
        useCase.updateMode(command);

        assertTrue(state.getMockExamState().getGroupOrder().isEmpty());
        assertTrue(state.getMockExamState().getGroupCounts().isEmpty());
    }

    @Test
    void updateMode_resetsSequentialProgressForSelectedList() {
        SessionState state = new SessionState(SessionId.of("s6"));
        QuestionList list = QuestionList.rehydrate(
                QuestionListId.of("list-3"),
                "list",
                List.of(
                        QuestionItem.rehydrate(QuestionItemId.of("q1"), "Q1", QuestionGroup.of("A")),
                        QuestionItem.rehydrate(QuestionItemId.of("q2"), "Q2", QuestionGroup.of("B"))
                ),
                Instant.parse("2026-01-01T00:00:00Z"),
                Instant.parse("2026-01-01T00:00:00Z")
        );
        state.nextQuestion(list);
        state.nextQuestion(list);
        assertTrue(state.nextQuestion(list).isEmpty());

        when(sessionStore.getOrCreate(SessionId.of("s6"))).thenReturn(state);
        when(questionListPort.findById(QuestionListId.of("list-3"))).thenReturn(Optional.of(list));

        ModeUpdateCommand command = command("s6", "list-3", ModeType.IMMEDIATE, null, null, null);
        useCase.updateMode(command);

        assertTrue(state.nextQuestion(list).isPresent());
    }
}
