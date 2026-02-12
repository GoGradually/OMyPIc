package me.go_gradually.omypic.application.session.usecase;

import me.go_gradually.omypic.application.question.port.QuestionGroupPort;
import me.go_gradually.omypic.application.session.model.InvalidGroupTagsException;
import me.go_gradually.omypic.application.session.model.ModeUpdateCommand;
import me.go_gradually.omypic.application.session.port.SessionStorePort;
import me.go_gradually.omypic.domain.question.QuestionGroupAggregate;
import me.go_gradually.omypic.domain.question.QuestionGroupId;
import me.go_gradually.omypic.domain.question.QuestionItem;
import me.go_gradually.omypic.domain.question.QuestionItemId;
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

import static org.junit.jupiter.api.Assertions.*;
import static org.mockito.Mockito.never;
import static org.mockito.Mockito.verify;
import static org.mockito.Mockito.when;

@ExtendWith(MockitoExtension.class)
class SessionUseCaseTest {

    @Mock
    private SessionStorePort sessionStore;
    @Mock
    private QuestionGroupPort questionGroupPort;

    private SessionUseCase useCase;

    @BeforeEach
    void setUp() {
        useCase = new SessionUseCase(sessionStore, questionGroupPort);
    }

    @Test
    void updateMode_clampsContinuousBatchSizeBetweenOneAndTen() {
        SessionState state = new SessionState(SessionId.of("s1"));
        when(sessionStore.getOrCreate(SessionId.of("s1"))).thenReturn(state);
        when(questionGroupPort.findAll()).thenReturn(List.of(group("g1", List.of("travel"), true)));

        ModeUpdateCommand upper = command("s1", ModeType.CONTINUOUS, 100, List.of("travel"));
        useCase.updateMode(upper);
        assertEquals(10, state.getContinuousBatchSize());

        ModeUpdateCommand lower = command("s1", ModeType.CONTINUOUS, 0, List.of("travel"));
        useCase.updateMode(lower);
        assertEquals(1, state.getContinuousBatchSize());
    }

    @Test
    void updateMode_configuresCandidateGroupsForSelectedTags() {
        SessionState state = new SessionState(SessionId.of("s2"));
        when(sessionStore.getOrCreate(SessionId.of("s2"))).thenReturn(state);
        when(questionGroupPort.findAll()).thenReturn(List.of(
                group("g1", List.of("travel"), true),
                group("g2", List.of("habit"), true),
                group("g3", List.of("travel"), false)
        ));

        SessionState updated = useCase.updateMode(command("s2", ModeType.IMMEDIATE, null, List.of(" travel ")));

        assertEquals(java.util.Set.of("travel"), updated.getSelectedGroupTags());
        assertEquals(1, updated.getCandidateGroupOrder().size());
        assertEquals("g1", updated.getCandidateGroupOrder().get(0));
    }

    @Test
    void updateMode_throwsWhenSelectedTagsAreEmpty() {
        SessionState state = new SessionState(SessionId.of("s3"));
        when(sessionStore.getOrCreate(SessionId.of("s3"))).thenReturn(state);

        InvalidGroupTagsException error = assertThrows(
                InvalidGroupTagsException.class,
                () -> useCase.updateMode(command("s3", ModeType.IMMEDIATE, null, List.of()))
        );

        assertEquals(List.of(), error.getInvalidTags());
        verify(questionGroupPort, never()).findAll();
    }

    @Test
    void updateMode_throwsWhenUnknownTagsAreIncluded() {
        SessionState state = new SessionState(SessionId.of("s4"));
        when(sessionStore.getOrCreate(SessionId.of("s4"))).thenReturn(state);
        when(questionGroupPort.findAll()).thenReturn(List.of(group("g1", List.of("travel"), true)));

        InvalidGroupTagsException error = assertThrows(
                InvalidGroupTagsException.class,
                () -> useCase.updateMode(command("s4", ModeType.IMMEDIATE, null, List.of("travel", "unknown")))
        );

        assertEquals(List.of("unknown"), error.getInvalidTags());
    }

    @Test
    void appendSegment_appendsTextToSession() {
        SessionState state = new SessionState(SessionId.of("s5"));
        when(sessionStore.getOrCreate(SessionId.of("s5"))).thenReturn(state);

        useCase.appendSegment("s5", "hello");

        assertEquals(1, state.getSttSegments().size());
        assertEquals("hello", state.getSttSegments().get(0));
    }

    private static ModeUpdateCommand command(String sessionId,
                                             ModeType mode,
                                             Integer batch,
                                             List<String> selectedTags) {
        ModeUpdateCommand command = new ModeUpdateCommand();
        command.setSessionId(sessionId);
        command.setMode(mode);
        command.setContinuousBatchSize(batch);
        command.setSelectedGroupTags(selectedTags);
        return command;
    }

    private QuestionGroupAggregate group(String id, List<String> tags, boolean withQuestions) {
        List<QuestionItem> questions = withQuestions
                ? List.of(QuestionItem.rehydrate(QuestionItemId.of("q-" + id), "Q", null))
                : List.of();
        return QuestionGroupAggregate.rehydrate(
                QuestionGroupId.of(id),
                id,
                tags,
                questions,
                Instant.parse("2026-01-01T00:00:00Z"),
                Instant.parse("2026-01-01T00:00:00Z")
        );
    }
}
