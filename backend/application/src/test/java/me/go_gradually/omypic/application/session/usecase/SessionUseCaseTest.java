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
import java.util.Optional;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;
import static org.mockito.ArgumentMatchers.any;
import static org.mockito.Mockito.never;
import static org.mockito.Mockito.verify;
import static org.mockito.Mockito.when;

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
            Integer batch
    ) {
        ModeUpdateCommand command = new ModeUpdateCommand();
        command.setSessionId(sessionId);
        command.setListId(listId);
        command.setMode(mode);
        command.setContinuousBatchSize(batch);
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

        ModeUpdateCommand upper = command("s1", null, ModeType.CONTINUOUS, 100);
        useCase.updateMode(upper);
        assertEquals(10, state.getContinuousBatchSize());

        ModeUpdateCommand lower = command("s1", null, ModeType.CONTINUOUS, 0);
        useCase.updateMode(lower);
        assertEquals(1, state.getContinuousBatchSize());
    }

    @Test
    void updateMode_withList_setsActiveListAndResetsSequentialProgress() {
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
        state.nextQuestion(list);
        state.nextQuestion(list);
        assertTrue(state.nextQuestion(list).isEmpty());

        when(sessionStore.getOrCreate(SessionId.of("s2"))).thenReturn(state);
        when(questionListPort.findById(QuestionListId.of("list-1"))).thenReturn(Optional.of(list));

        ModeUpdateCommand command = command("s2", "list-1", ModeType.IMMEDIATE, null);
        SessionState updated = useCase.updateMode(command);

        assertEquals("list-1", updated.getActiveQuestionListId());
        assertTrue(updated.nextQuestion(list).isPresent());
    }

    @Test
    void updateMode_doesNotLookupList_whenListIdIsNull() {
        SessionState state = new SessionState(SessionId.of("s3"));
        when(sessionStore.getOrCreate(SessionId.of("s3"))).thenReturn(state);

        ModeUpdateCommand command = command("s3", null, ModeType.IMMEDIATE, null);
        useCase.updateMode(command);

        verify(questionListPort, never()).findById(any());
    }

    @Test
    void appendSegment_appendsTextToSession() {
        SessionState state = new SessionState(SessionId.of("s4"));
        when(sessionStore.getOrCreate(SessionId.of("s4"))).thenReturn(state);

        useCase.appendSegment("s4", "hello");

        assertEquals(1, state.getSttSegments().size());
        assertEquals("hello", state.getSttSegments().get(0));
    }
}
