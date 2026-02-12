package me.go_gradually.omypic.infrastructure.question.persistence.mongo;

import me.go_gradually.omypic.domain.question.QuestionGroupAggregate;
import me.go_gradually.omypic.domain.question.QuestionGroupId;
import me.go_gradually.omypic.domain.question.QuestionItem;
import me.go_gradually.omypic.domain.question.QuestionItemId;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.extension.ExtendWith;
import org.mockito.ArgumentCaptor;
import org.mockito.Mock;
import org.mockito.junit.jupiter.MockitoExtension;

import java.time.Instant;
import java.util.List;
import java.util.Optional;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;
import static org.mockito.ArgumentMatchers.any;
import static org.mockito.Mockito.verify;
import static org.mockito.Mockito.when;

@ExtendWith(MockitoExtension.class)
class QuestionGroupMongoAdapterTest {

    @Mock
    private QuestionGroupMongoRepository repository;

    private QuestionGroupMongoAdapter adapter;

    @BeforeEach
    void setUp() {
        adapter = new QuestionGroupMongoAdapter(repository);
    }

    @Test
    void findAll_mapsDocumentsToDomain() {
        when(repository.findAll()).thenReturn(List.of(documentWithQuestions()));

        List<QuestionGroupAggregate> result = adapter.findAll();

        assertEquals(1, result.size());
        assertEquals("g1", result.get(0).getId().value());
        assertEquals("Travel", result.get(0).getName());
        assertEquals("q1", result.get(0).getQuestions().get(0).getId().value());
    }

    @Test
    void findById_returnsEmpty_whenMissing() {
        when(repository.findById("missing")).thenReturn(Optional.empty());

        Optional<QuestionGroupAggregate> result = adapter.findById(QuestionGroupId.of("missing"));

        assertTrue(result.isEmpty());
    }

    @Test
    void save_mapsDomainToDocumentAndBack() {
        QuestionGroupAggregate group = QuestionGroupAggregate.rehydrate(
                QuestionGroupId.of("g1"),
                "Travel",
                List.of("travel", "habit"),
                List.of(QuestionItem.rehydrate(QuestionItemId.of("q1"), "Question", "habit")),
                Instant.parse("2026-02-01T00:00:00Z"),
                Instant.parse("2026-02-01T00:00:00Z")
        );
        when(repository.save(any(QuestionGroupDocument.class))).thenAnswer(invocation -> invocation.getArgument(0));

        QuestionGroupAggregate saved = adapter.save(group);

        ArgumentCaptor<QuestionGroupDocument> captor = ArgumentCaptor.forClass(QuestionGroupDocument.class);
        verify(repository).save(captor.capture());
        assertEquals("g1", captor.getValue().getId());
        assertEquals("Travel", captor.getValue().getName());
        assertEquals("q1", captor.getValue().getQuestions().get(0).getId());
        assertEquals("habit", captor.getValue().getQuestions().get(0).getQuestionType());

        assertEquals("g1", saved.getId().value());
        assertEquals("Question", saved.getQuestions().get(0).getText());
        assertEquals("habit", saved.getQuestions().get(0).getQuestionType());
    }

    @Test
    void deleteById_delegatesToRepository() {
        adapter.deleteById(QuestionGroupId.of("g2"));

        verify(repository).deleteById("g2");
    }

    private QuestionGroupDocument documentWithQuestions() {
        QuestionItemDocument item = new QuestionItemDocument();
        item.setId("q1");
        item.setText("Question");
        item.setQuestionType("habit");

        QuestionGroupDocument doc = new QuestionGroupDocument();
        doc.setId("g1");
        doc.setName("Travel");
        doc.setTags(List.of("travel", "habit"));
        doc.setQuestions(List.of(item));
        doc.setCreatedAt(Instant.parse("2026-02-01T00:00:00Z"));
        doc.setUpdatedAt(Instant.parse("2026-02-01T00:00:00Z"));
        return doc;
    }
}
