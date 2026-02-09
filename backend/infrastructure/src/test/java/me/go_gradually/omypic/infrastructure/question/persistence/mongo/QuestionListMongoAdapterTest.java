package me.go_gradually.omypic.infrastructure.question.persistence.mongo;

import me.go_gradually.omypic.domain.question.QuestionItem;
import me.go_gradually.omypic.domain.question.QuestionItemId;
import me.go_gradually.omypic.domain.question.QuestionList;
import me.go_gradually.omypic.domain.question.QuestionListId;
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
class QuestionListMongoAdapterTest {

    @Mock
    private QuestionListMongoRepository repository;

    private QuestionListMongoAdapter adapter;

    @BeforeEach
    void setUp() {
        adapter = new QuestionListMongoAdapter(repository);
    }

    @Test
    void findAll_mapsDocumentsToDomain() {
        when(repository.findAll()).thenReturn(List.of(documentWithQuestions()));

        List<QuestionList> result = adapter.findAll();

        assertEquals(1, result.size());
        assertEquals("l1", result.get(0).getId().value());
        assertEquals("List", result.get(0).getName());
        assertEquals("q1", result.get(0).getQuestions().get(0).getId().value());
    }

    @Test
    void findById_returnsEmpty_whenMissing() {
        when(repository.findById("missing")).thenReturn(Optional.empty());

        Optional<QuestionList> result = adapter.findById(QuestionListId.of("missing"));

        assertTrue(result.isEmpty());
    }

    @Test
    void findById_handlesNullQuestionsAsEmptyList() {
        QuestionListDocument doc = documentWithQuestions();
        doc.setQuestions(null);
        when(repository.findById("l1")).thenReturn(Optional.of(doc));

        Optional<QuestionList> result = adapter.findById(QuestionListId.of("l1"));

        assertTrue(result.isPresent());
        assertTrue(result.get().getQuestions().isEmpty());
    }

    @Test
    void save_mapsDomainToDocumentAndBack() {
        QuestionList list = QuestionList.rehydrate(
                QuestionListId.of("l1"),
                "List",
                List.of(QuestionItem.rehydrate(QuestionItemId.of("q1"), "Question", "A")),
                Instant.parse("2026-02-01T00:00:00Z"),
                Instant.parse("2026-02-01T00:00:00Z")
        );
        when(repository.save(any(QuestionListDocument.class))).thenAnswer(invocation -> invocation.getArgument(0));

        QuestionList saved = adapter.save(list);

        ArgumentCaptor<QuestionListDocument> captor = ArgumentCaptor.forClass(QuestionListDocument.class);
        verify(repository).save(captor.capture());
        assertEquals("l1", captor.getValue().getId());
        assertEquals("List", captor.getValue().getName());
        assertEquals("q1", captor.getValue().getQuestions().get(0).getId());

        assertEquals("l1", saved.getId().value());
        assertEquals("Question", saved.getQuestions().get(0).getText());
    }

    @Test
    void deleteById_delegatesToRepository() {
        adapter.deleteById(QuestionListId.of("l2"));

        verify(repository).deleteById("l2");
    }

    private QuestionListDocument documentWithQuestions() {
        QuestionItemDocument item = new QuestionItemDocument();
        item.setId("q1");
        item.setText("Question");
        item.setGroup("A");

        QuestionListDocument doc = new QuestionListDocument();
        doc.setId("l1");
        doc.setName("List");
        doc.setQuestions(List.of(item));
        doc.setCreatedAt(Instant.parse("2026-02-01T00:00:00Z"));
        doc.setUpdatedAt(Instant.parse("2026-02-01T00:00:00Z"));
        return doc;
    }
}
