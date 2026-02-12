package me.go_gradually.omypic.application.rulebook.usecase;

import me.go_gradually.omypic.application.rulebook.model.StoredRulebookFile;
import me.go_gradually.omypic.application.rulebook.policy.RagPolicy;
import me.go_gradually.omypic.application.rulebook.port.RulebookFileStore;
import me.go_gradually.omypic.application.rulebook.port.RulebookIndexPort;
import me.go_gradually.omypic.application.rulebook.port.RulebookPort;
import me.go_gradually.omypic.application.shared.port.MetricsPort;
import me.go_gradually.omypic.domain.question.QuestionGroup;
import me.go_gradually.omypic.domain.rulebook.Rulebook;
import me.go_gradually.omypic.domain.rulebook.RulebookContext;
import me.go_gradually.omypic.domain.rulebook.RulebookId;
import me.go_gradually.omypic.domain.rulebook.RulebookScope;
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
import java.util.Set;

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
    @Mock
    private MetricsPort metrics;

    private RulebookUseCase useCase;

    @BeforeEach
    void setUp() {
        useCase = new RulebookUseCase(repository, indexPort, fileStore, ragPolicy, metrics);
    }

    @Test
    void upload_rejectsNonMarkdownFiles() {
        assertThrows(IllegalArgumentException.class, () -> useCase.upload("rulebook.txt", new byte[]{1}, RulebookScope.MAIN, null));
    }

    @Test
    void upload_rejectsQuestionGroupInMainScope() {
        assertThrows(IllegalArgumentException.class, () -> useCase.upload("rulebook.md", new byte[]{1}, RulebookScope.MAIN, "A"));
    }

    @Test
    void upload_requiresQuestionGroupInQuestionScope() {
        assertThrows(IllegalArgumentException.class, () -> useCase.upload("rulebook.md", new byte[]{1}, RulebookScope.QUESTION, " "));
    }

    @Test
    void upload_storesSavesReadsAndIndexes() throws IOException {
        byte[] bytes = "# Rulebook\nhello".getBytes();
        StoredRulebookFile stored = new StoredRulebookFile("/tmp/rulebook.md");

        when(fileStore.store("rulebook.md", bytes)).thenReturn(stored);
        when(repository.save(any(Rulebook.class))).thenAnswer(invocation -> invocation.getArgument(0));
        when(fileStore.readText("/tmp/rulebook.md")).thenReturn("line1\nline2\nline3");

        Rulebook saved = useCase.upload("rulebook.md", bytes, RulebookScope.QUESTION, "A");

        assertEquals("rulebook.md", saved.getFilename());
        assertEquals("/tmp/rulebook.md", saved.getPath());
        assertEquals(RulebookScope.QUESTION, saved.getScope());
        assertEquals(QuestionGroup.of("A"), saved.getQuestionGroup());
        verify(fileStore).store("rulebook.md", bytes);
        verify(fileStore).readText("/tmp/rulebook.md");

        @SuppressWarnings("unchecked")
        ArgumentCaptor<List<String>> chunksCaptor = ArgumentCaptor.forClass(List.class);
        verify(indexPort).indexRulebookChunks(eq(saved.getId()), eq("rulebook.md"), chunksCaptor.capture());
        assertTrue(chunksCaptor.getValue().size() >= 1);
        verify(metrics).recordRulebookUploadLatency(any());
    }

    @Test
    void searchContexts_returnsEmpty_whenNoEnabledRulebooks() {
        Rulebook disabled = Rulebook.rehydrate(
                RulebookId.of("r1"),
                "rulebook.md",
                "/tmp/rulebook.md",
                RulebookScope.MAIN,
                null,
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
                RulebookScope.MAIN,
                null,
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
                RulebookScope.MAIN,
                null,
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

    @Test
    void searchContextsForTurn_prioritizesMainAndQuestionAndLimitsToTwo() throws IOException {
        Rulebook main = Rulebook.rehydrate(
                RulebookId.of("r-main"),
                "main.md",
                "/tmp/main.md",
                RulebookScope.MAIN,
                null,
                true,
                Instant.parse("2026-01-01T00:00:00Z"),
                Instant.parse("2026-01-01T00:00:00Z")
        );
        Rulebook questionA = Rulebook.rehydrate(
                RulebookId.of("r-q-a"),
                "q-a.md",
                "/tmp/q-a.md",
                RulebookScope.QUESTION,
                QuestionGroup.of("A"),
                true,
                Instant.parse("2026-01-01T00:00:00Z"),
                Instant.parse("2026-01-01T00:00:00Z")
        );
        Rulebook questionB = Rulebook.rehydrate(
                RulebookId.of("r-q-b"),
                "q-b.md",
                "/tmp/q-b.md",
                RulebookScope.QUESTION,
                QuestionGroup.of("B"),
                true,
                Instant.parse("2026-01-01T00:00:00Z"),
                Instant.parse("2026-01-01T00:00:00Z")
        );
        when(repository.findAll()).thenReturn(List.of(main, questionA, questionB));
        when(indexPort.search(eq("answer"), eq(1), eq(Set.of(RulebookId.of("r-main")))))
                .thenReturn(List.of(RulebookContext.of(RulebookId.of("r-main"), "main.md", "main ctx")));
        when(indexPort.search(eq("answer"), eq(1), eq(Set.of(RulebookId.of("r-q-a")))))
                .thenReturn(List.of(RulebookContext.of(RulebookId.of("r-q-a"), "q-a.md", "group A ctx")));
        when(indexPort.search(eq("answer"), eq(2), eq(Set.of(RulebookId.of("r-main")))))
                .thenReturn(List.of(RulebookContext.of(RulebookId.of("r-main"), "main.md", "main ctx")));
        when(indexPort.search(eq("answer"), eq(2), eq(Set.of(RulebookId.of("r-q-a")))))
                .thenReturn(List.of(RulebookContext.of(RulebookId.of("r-q-a"), "q-a.md", "group A ctx")));

        List<RulebookContext> contexts = useCase.searchContextsForTurn(QuestionGroup.of("A"), "answer", 2);

        assertEquals(2, contexts.size());
        assertEquals("r-main", contexts.get(0).rulebookId().value());
        assertEquals("r-q-a", contexts.get(1).rulebookId().value());
    }
}
