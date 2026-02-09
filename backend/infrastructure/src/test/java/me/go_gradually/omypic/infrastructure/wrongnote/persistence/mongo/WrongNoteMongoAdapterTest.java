package me.go_gradually.omypic.infrastructure.wrongnote.persistence.mongo;

import me.go_gradually.omypic.domain.wrongnote.WrongNote;
import me.go_gradually.omypic.domain.wrongnote.WrongNoteId;
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
class WrongNoteMongoAdapterTest {

    @Mock
    private WrongNoteMongoRepository repository;

    private WrongNoteMongoAdapter adapter;

    @BeforeEach
    void setUp() {
        adapter = new WrongNoteMongoAdapter(repository);
    }

    @Test
    void findAll_mapsDocumentsToDomain() {
        when(repository.findAll()).thenReturn(List.of(document()));

        List<WrongNote> result = adapter.findAll();

        assertEquals(1, result.size());
        assertEquals("n1", result.get(0).getId().value());
        assertEquals("pattern", result.get(0).getPattern());
        assertEquals(2, result.get(0).getCount());
    }

    @Test
    void findByPattern_returnsOptionalDomain() {
        when(repository.findByPattern("pattern")).thenReturn(Optional.of(document()));

        Optional<WrongNote> result = adapter.findByPattern("pattern");

        assertTrue(result.isPresent());
        assertEquals("summary", result.get().getShortSummary());
    }

    @Test
    void save_mapsDomainToDocumentAndBack() {
        WrongNote note = WrongNote.rehydrate(
                WrongNoteId.of("n1"),
                "pattern",
                2,
                "summary",
                Instant.parse("2026-02-01T00:00:00Z")
        );
        when(repository.save(any(WrongNoteDocument.class))).thenAnswer(invocation -> invocation.getArgument(0));

        WrongNote saved = adapter.save(note);

        ArgumentCaptor<WrongNoteDocument> captor = ArgumentCaptor.forClass(WrongNoteDocument.class);
        verify(repository).save(captor.capture());
        assertEquals("n1", captor.getValue().getId());
        assertEquals("pattern", captor.getValue().getPattern());
        assertEquals(2, captor.getValue().getCount());

        assertEquals("n1", saved.getId().value());
        assertEquals("summary", saved.getShortSummary());
    }

    @Test
    void deleteById_delegatesToRepository() {
        adapter.deleteById(WrongNoteId.of("n2"));

        verify(repository).deleteById("n2");
    }

    private WrongNoteDocument document() {
        WrongNoteDocument doc = new WrongNoteDocument();
        doc.setId("n1");
        doc.setPattern("pattern");
        doc.setCount(2);
        doc.setShortSummary("summary");
        doc.setLastSeenAt(Instant.parse("2026-02-01T00:00:00Z"));
        return doc;
    }
}
