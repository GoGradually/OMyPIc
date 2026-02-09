package me.go_gradually.omypic.application.rulebook.usecase;

import me.go_gradually.omypic.application.rulebook.model.StoredRulebookFile;
import me.go_gradually.omypic.application.rulebook.policy.RagPolicy;
import me.go_gradually.omypic.application.rulebook.port.RulebookFileStore;
import me.go_gradually.omypic.application.rulebook.port.RulebookIndexPort;
import me.go_gradually.omypic.application.rulebook.port.RulebookPort;
import me.go_gradually.omypic.domain.rulebook.Rulebook;
import me.go_gradually.omypic.domain.rulebook.RulebookContext;
import me.go_gradually.omypic.domain.rulebook.RulebookId;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.extension.ExtendWith;
import org.mockito.ArgumentCaptor;
import org.mockito.Mock;
import org.mockito.junit.jupiter.MockitoExtension;

import java.io.IOException;
import java.time.Instant;
import java.util.List;
import java.util.Optional;

import static org.junit.jupiter.api.Assertions.*;
import static org.mockito.ArgumentMatchers.*;
import static org.mockito.Mockito.*;

@ExtendWith(MockitoExtension.class)
class RulebookUseCaseTest {

    @Mock
    private RulebookPort repository;
    @Mock
    private RulebookIndexPort indexPort;
    @Mock
    private RulebookFileStore fileStore;
    @Mock
    private RagPolicy ragPolicy;

    private RulebookUseCase useCase;

    @BeforeEach
    void setUp() {
        useCase = new RulebookUseCase(repository, indexPort, fileStore, ragPolicy);
    }

    @Test
    void upload_rejectsNonMarkdownFiles() {
        assertThrows(IllegalArgumentException.class, () -> useCase.upload("rulebook.txt", new byte[]{1}));
    }

    @Test
    void upload_storesSavesReadsAndIndexes() throws IOException {
        byte[] bytes = "# Rulebook\nhello".getBytes();
        StoredRulebookFile stored = new StoredRulebookFile("/tmp/rulebook.md");

        when(fileStore.store("rulebook.md", bytes)).thenReturn(stored);
        when(repository.save(any(Rulebook.class))).thenAnswer(invocation -> invocation.getArgument(0));
        when(fileStore.readText("/tmp/rulebook.md")).thenReturn("line1\nline2\nline3");

        Rulebook saved = useCase.upload("rulebook.md", bytes);

        assertEquals("rulebook.md", saved.getFilename());
        assertEquals("/tmp/rulebook.md", saved.getPath());
        verify(fileStore).store("rulebook.md", bytes);
        verify(fileStore).readText("/tmp/rulebook.md");

        @SuppressWarnings("unchecked")
        ArgumentCaptor<List<String>> chunksCaptor = ArgumentCaptor.forClass(List.class);
        verify(indexPort).indexRulebookChunks(eq(saved.getId()), eq("rulebook.md"), chunksCaptor.capture());
        assertTrue(chunksCaptor.getValue().size() >= 1);
    }

    @Test
    void searchContexts_returnsEmpty_whenNoEnabledRulebooks() {
        Rulebook disabled = Rulebook.rehydrate(
                RulebookId.of("r1"),
                "rulebook.md",
                "/tmp/rulebook.md",
                false,
                Instant.parse("2026-01-01T00:00:00Z"),
                Instant.parse("2026-01-01T00:00:00Z")
        );
        when(repository.findAll()).thenReturn(List.of(disabled));

        List<RulebookContext> contexts = useCase.searchContexts("grammar");

        assertTrue(contexts.isEmpty());
        verifyNoInteractions(indexPort);
    }

    @Test
    void searchContexts_returnsEmpty_whenIndexThrowsIOException() throws IOException {
        Rulebook enabled = Rulebook.rehydrate(
                RulebookId.of("r1"),
                "rulebook.md",
                "/tmp/rulebook.md",
                true,
                Instant.parse("2026-01-01T00:00:00Z"),
                Instant.parse("2026-01-01T00:00:00Z")
        );
        when(repository.findAll()).thenReturn(List.of(enabled));
        when(ragPolicy.getMaxContextChunks()).thenReturn(3);
        when(indexPort.search(eq("grammar"), eq(3), anySet())).thenThrow(new IOException("index error"));

        List<RulebookContext> contexts = useCase.searchContexts("grammar");

        assertTrue(contexts.isEmpty());
    }

    @Test
    void toggle_updatesEnabledStateAndSaves() {
        Rulebook existing = Rulebook.rehydrate(
                RulebookId.of("r1"),
                "rulebook.md",
                "/tmp/rulebook.md",
                true,
                Instant.parse("2026-01-01T00:00:00Z"),
                Instant.parse("2026-01-01T00:00:00Z")
        );
        when(repository.findById(RulebookId.of("r1"))).thenReturn(Optional.of(existing));
        when(repository.save(existing)).thenReturn(existing);

        Rulebook toggled = useCase.toggle("r1", false);

        assertFalse(toggled.isEnabled());
        verify(repository).save(existing);
    }

    @Test
    void delete_delegatesToRepository() {
        useCase.delete("r1");

        verify(repository).deleteById(RulebookId.of("r1"));
    }
}
