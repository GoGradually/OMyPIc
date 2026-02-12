package me.go_gradually.omypic.application.question.usecase;

import me.go_gradually.omypic.application.question.model.NextQuestion;
import me.go_gradually.omypic.application.question.model.QuestionTagStat;
import me.go_gradually.omypic.application.question.port.QuestionGroupPort;
import me.go_gradually.omypic.application.session.port.SessionStorePort;
import me.go_gradually.omypic.application.shared.port.MetricsPort;
import me.go_gradually.omypic.domain.question.QuestionGroupAggregate;
import me.go_gradually.omypic.domain.question.QuestionGroupId;
import me.go_gradually.omypic.domain.question.QuestionItem;
import me.go_gradually.omypic.domain.question.QuestionItemId;
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
import static org.mockito.ArgumentMatchers.any;
import static org.mockito.Mockito.*;

@ExtendWith(MockitoExtension.class)
class QuestionUseCaseTest {

    @Mock
    private QuestionGroupPort repository;
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
    void createGroup_savesGroupWithNormalizedTags() {
        when(repository.save(any(QuestionGroupAggregate.class))).thenAnswer(invocation -> invocation.getArgument(0));

        QuestionGroupAggregate created = useCase.createGroup("travel", List.of(" Travel ", "habit"));

        assertEquals("travel", created.getName());
        assertTrue(created.getTags().contains("travel"));
        assertTrue(created.getTags().contains("habit"));
        verify(repository).save(any(QuestionGroupAggregate.class));
    }

    @Test
    void listTagStats_returnsSortedCounts() {
        QuestionGroupAggregate g1 = group("g1", "A", List.of("travel", "habit"), List.of(item("q1", "Q1", "habit")));
        QuestionGroupAggregate g2 = group("g2", "B", List.of("travel"), List.of(item("q2", "Q2", "compare")));
        when(repository.findAll()).thenReturn(List.of(g1, g2));

        List<QuestionTagStat> stats = useCase.listTagStats();

        assertEquals(2, stats.size());
        assertEquals("habit", stats.get(0).tag());
        assertEquals(1, stats.get(0).groupCount());
        assertTrue(stats.get(0).selectable());
        assertEquals("travel", stats.get(1).tag());
        assertEquals(2, stats.get(1).groupCount());
    }

    @Test
    void nextQuestion_usesSessionCandidateGroupOrderAndReturnsSkippedWhenExhausted() {
        QuestionGroupAggregate g1 = group("g1", "Travel", List.of("travel"), List.of(
                item("q1", "Q1", "habit"),
                item("q2", "Q2", "compare")
        ));
        QuestionGroupAggregate g2 = group("g2", "Cafe", List.of("travel"), List.of(
                item("q3", "Q3", null)
        ));

        SessionState state = new SessionState(SessionId.of("s1"));
        state.configureQuestionGroups(java.util.Set.of("travel"), List.of("g1", "g2"));

        when(sessionStore.getOrCreate(SessionId.of("s1"))).thenReturn(state);
        when(repository.findAll()).thenReturn(List.of(g1, g2));

        NextQuestion first = useCase.nextQuestion("s1");
        NextQuestion second = useCase.nextQuestion("s1");
        NextQuestion third = useCase.nextQuestion("s1");
        NextQuestion fourth = useCase.nextQuestion("s1");

        assertEquals("q1", first.getQuestionId());
        assertEquals("g1", first.getGroupId());
        assertEquals("habit", first.getQuestionType());

        assertEquals("q2", second.getQuestionId());
        assertEquals("q3", third.getQuestionId());
        assertTrue(fourth.isSkipped());
        assertEquals(List.of("g1", "g2"), state.getCandidateGroupOrder(), "candidateGroupOrder must remain fixed within a session");

        verify(metrics, times(4)).recordQuestionNextLatency(any());
    }

    @Test
    void nextQuestion_throwsWhenSelectedTagsAreMissing() {
        SessionState state = new SessionState(SessionId.of("s2"));
        when(sessionStore.getOrCreate(SessionId.of("s2"))).thenReturn(state);

        assertThrows(IllegalStateException.class, () -> useCase.nextQuestion("s2"));
    }

    private QuestionGroupAggregate group(String id, String name, List<String> tags, List<QuestionItem> questions) {
        return QuestionGroupAggregate.rehydrate(
                QuestionGroupId.of(id),
                name,
                tags,
                questions,
                Instant.parse("2026-01-01T00:00:00Z"),
                Instant.parse("2026-01-01T00:00:00Z")
        );
    }

    private QuestionItem item(String id, String text, String questionType) {
        return QuestionItem.rehydrate(QuestionItemId.of(id), text, questionType);
    }
}
