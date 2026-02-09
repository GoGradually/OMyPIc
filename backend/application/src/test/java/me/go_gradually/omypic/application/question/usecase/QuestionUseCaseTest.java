package me.go_gradually.omypic.application.question.usecase;

import me.go_gradually.omypic.application.question.model.NextQuestion;
import me.go_gradually.omypic.application.question.port.QuestionListPort;
import me.go_gradually.omypic.application.session.port.SessionStorePort;
import me.go_gradually.omypic.application.shared.port.MetricsPort;
import me.go_gradually.omypic.domain.question.QuestionItem;
import me.go_gradually.omypic.domain.question.QuestionItemId;
import me.go_gradually.omypic.domain.question.QuestionList;
import me.go_gradually.omypic.domain.question.QuestionListId;
import me.go_gradually.omypic.domain.session.SessionId;
import me.go_gradually.omypic.domain.session.SessionState;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.extension.ExtendWith;
import org.mockito.Mock;
import org.mockito.junit.jupiter.MockitoExtension;

import java.time.Instant;
import java.util.List;
import java.util.NoSuchElementException;
import java.util.Optional;

import static org.junit.jupiter.api.Assertions.*;
import static org.mockito.ArgumentMatchers.any;
import static org.mockito.Mockito.*;

@ExtendWith(MockitoExtension.class)
class QuestionUseCaseTest {

    @Mock
    private QuestionListPort repository;
    @Mock
    private SessionStorePort sessionStore;
    @Mock
    private MetricsPort metrics;

    private QuestionUseCase useCase;

    @BeforeEach
    void setUp() {
        useCase = new QuestionUseCase(repository, sessionStore, metrics);
    }

    @Test
    void list_returnsFromRepository() {
        QuestionList list = QuestionList.create("list", Instant.parse("2026-01-01T00:00:00Z"));
        when(repository.findAll()).thenReturn(List.of(list));

        List<QuestionList> result = useCase.list();

        assertEquals(1, result.size());
    }

    @Test
    void create_savesNewList() {
        when(repository.save(any(QuestionList.class))).thenAnswer(invocation -> invocation.getArgument(0));

        QuestionList created = useCase.create("my-list");

        assertEquals("my-list", created.getName());
        verify(repository).save(any(QuestionList.class));
    }

    @Test
    void updateName_updatesAndSaves() {
        QuestionListId listId = QuestionListId.of("list-1");
        QuestionList list = QuestionList.rehydrate(
                listId,
                "before",
                List.of(),
                Instant.parse("2026-01-01T00:00:00Z"),
                Instant.parse("2026-01-01T00:00:00Z")
        );
        when(repository.findById(listId)).thenReturn(Optional.of(list));
        when(repository.save(list)).thenReturn(list);

        QuestionList updated = useCase.updateName("list-1", "after");

        assertEquals("after", updated.getName());
        verify(repository).save(list);
    }

    @Test
    void delete_delegatesToRepository() {
        useCase.delete("list-1");

        verify(repository).deleteById(QuestionListId.of("list-1"));
    }

    @Test
    void addUpdateDeleteQuestion_updatesListAndSaves() {
        QuestionListId listId = QuestionListId.of("list-1");
        QuestionItem existing = QuestionItem.rehydrate(QuestionItemId.of("q1"), "Q1", "A");
        QuestionList list = QuestionList.rehydrate(
                listId,
                "list",
                List.of(existing),
                Instant.parse("2026-01-01T00:00:00Z"),
                Instant.parse("2026-01-01T00:00:00Z")
        );
        when(repository.findById(listId)).thenReturn(Optional.of(list));
        when(repository.save(list)).thenReturn(list);

        useCase.addQuestion("list-1", "Q2", "B");
        useCase.updateQuestion("list-1", "q1", "Q1 updated", "C");
        QuestionList afterDelete = useCase.deleteQuestion("list-1", "q1");

        assertTrue(afterDelete.getQuestions().stream().noneMatch(q -> q.getId().equals(QuestionItemId.of("q1"))));
        verify(repository, times(3)).save(list);
    }

    @Test
    void nextQuestion_cyclesThroughListInOrder() {
        QuestionListId listId = QuestionListId.of("list-1");
        QuestionList list = QuestionList.rehydrate(
                listId,
                "list",
                List.of(
                        QuestionItem.rehydrate(QuestionItemId.of("q1"), "Q1", "A"),
                        QuestionItem.rehydrate(QuestionItemId.of("q2"), "Q2", "B")
                ),
                Instant.parse("2026-01-01T00:00:00Z"),
                Instant.parse("2026-01-01T00:00:00Z")
        );
        SessionState state = new SessionState(SessionId.of("s1"));
        when(repository.findById(listId)).thenReturn(Optional.of(list));
        when(sessionStore.getOrCreate(SessionId.of("s1"))).thenReturn(state);

        NextQuestion first = useCase.nextQuestion("list-1", "s1");
        NextQuestion second = useCase.nextQuestion("list-1", "s1");
        NextQuestion third = useCase.nextQuestion("list-1", "s1");

        assertEquals("q1", first.getQuestionId());
        assertEquals("q2", second.getQuestionId());
        assertEquals("q1", third.getQuestionId());
        verify(metrics, times(3)).recordQuestionNextLatency(any());
    }

    @Test
    void nextQuestion_returnsSkipped_whenListIsEmpty() {
        QuestionListId listId = QuestionListId.of("empty");
        QuestionList list = QuestionList.rehydrate(
                listId,
                "empty",
                List.of(),
                Instant.parse("2026-01-01T00:00:00Z"),
                Instant.parse("2026-01-01T00:00:00Z")
        );
        when(repository.findById(listId)).thenReturn(Optional.of(list));
        when(sessionStore.getOrCreate(SessionId.of("s1"))).thenReturn(new SessionState(SessionId.of("s1")));

        NextQuestion next = useCase.nextQuestion("empty", "s1");

        assertTrue(next.isSkipped());
    }

    @Test
    void nextQuestion_throwsWhenListDoesNotExist() {
        when(repository.findById(QuestionListId.of("missing"))).thenReturn(Optional.empty());

        assertThrows(NoSuchElementException.class, () -> useCase.nextQuestion("missing", "s1"));
    }
}
