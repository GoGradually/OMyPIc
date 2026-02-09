package me.go_gradually.omypic.infrastructure.rulebook.persistence.mongo;

import me.go_gradually.omypic.domain.rulebook.Rulebook;
import me.go_gradually.omypic.domain.rulebook.RulebookId;
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
class RulebookMongoAdapterTest {

    @Mock
    private RulebookMongoRepository repository;

    private RulebookMongoAdapter adapter;

    @BeforeEach
    void setUp() {
        adapter = new RulebookMongoAdapter(repository);
    }

    @Test
    void findAll_mapsDocumentsToDomain() {
        when(repository.findAll()).thenReturn(List.of(document()));

        List<Rulebook> result = adapter.findAll();

        assertEquals(1, result.size());
        assertEquals("r1", result.get(0).getId().value());
        assertEquals("rules.md", result.get(0).getFilename());
        assertTrue(result.get(0).isEnabled());
    }

    @Test
    void findById_returnsOptionalDomain() {
        when(repository.findById("r1")).thenReturn(Optional.of(document()));

        Optional<Rulebook> result = adapter.findById(RulebookId.of("r1"));

        assertTrue(result.isPresent());
        assertEquals("/tmp/rules.md", result.get().getPath());
    }

    @Test
    void save_mapsDomainToDocumentAndBack() {
        Rulebook rulebook = Rulebook.rehydrate(
                RulebookId.of("r1"),
                "rules.md",
                "/tmp/rules.md",
                true,
                Instant.parse("2026-02-01T00:00:00Z"),
                Instant.parse("2026-02-01T00:00:00Z")
        );
        when(repository.save(any(RulebookDocument.class))).thenAnswer(invocation -> invocation.getArgument(0));

        Rulebook saved = adapter.save(rulebook);

        ArgumentCaptor<RulebookDocument> captor = ArgumentCaptor.forClass(RulebookDocument.class);
        verify(repository).save(captor.capture());
        assertEquals("r1", captor.getValue().getId());
        assertEquals("rules.md", captor.getValue().getFilename());
        assertEquals("/tmp/rules.md", captor.getValue().getPath());

        assertEquals("r1", saved.getId().value());
        assertTrue(saved.isEnabled());
    }

    @Test
    void deleteById_delegatesToRepository() {
        adapter.deleteById(RulebookId.of("r2"));

        verify(repository).deleteById("r2");
    }

    private RulebookDocument document() {
        RulebookDocument doc = new RulebookDocument();
        doc.setId("r1");
        doc.setFilename("rules.md");
        doc.setPath("/tmp/rules.md");
        doc.setEnabled(true);
        doc.setCreatedAt(Instant.parse("2026-02-01T00:00:00Z"));
        doc.setUpdatedAt(Instant.parse("2026-02-01T00:00:00Z"));
        return doc;
    }
}
