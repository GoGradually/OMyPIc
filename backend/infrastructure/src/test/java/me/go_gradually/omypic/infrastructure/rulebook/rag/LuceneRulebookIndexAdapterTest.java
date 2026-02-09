package me.go_gradually.omypic.infrastructure.rulebook.rag;

import me.go_gradually.omypic.application.rulebook.port.EmbeddingPort;
import me.go_gradually.omypic.application.shared.policy.DataDirProvider;
import me.go_gradually.omypic.domain.rulebook.RulebookContext;
import me.go_gradually.omypic.domain.rulebook.RulebookId;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

import java.io.IOException;
import java.nio.file.Path;
import java.util.List;
import java.util.Set;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

class LuceneRulebookIndexAdapterTest {

    @TempDir
    Path tempDir;

    @Test
    void search_returnsEmpty_whenEnabledSetIsEmpty() throws IOException {
        LuceneRulebookIndexAdapter adapter = new LuceneRulebookIndexAdapter(dataDir(tempDir), embedding());

        List<RulebookContext> result = adapter.search("anything", 3, Set.of());

        assertTrue(result.isEmpty());
    }

    @Test
    void search_returnsEmpty_whenIndexDoesNotExist() throws IOException {
        LuceneRulebookIndexAdapter adapter = new LuceneRulebookIndexAdapter(dataDir(tempDir), embedding());

        List<RulebookContext> result = adapter.search("alpha", 3, Set.of(RulebookId.of("r1")));

        assertTrue(result.isEmpty());
    }

    @Test
    void indexAndSearch_filtersByEnabledRulebookIds_andRespectsTopK() throws IOException {
        LuceneRulebookIndexAdapter adapter = new LuceneRulebookIndexAdapter(dataDir(tempDir), embedding());

        adapter.indexRulebookChunks(RulebookId.of("r1"), "r1.md", List.of("alpha first", "alpha second"));
        adapter.indexRulebookChunks(RulebookId.of("r2"), "r2.md", List.of("beta only"));

        List<RulebookContext> result = adapter.search("alpha", 1, Set.of(RulebookId.of("r1")));

        assertEquals(1, result.size());
        assertEquals("r1", result.get(0).rulebookId().value());
        assertEquals("r1.md", result.get(0).filename());
        assertTrue(result.get(0).text().contains("alpha"));
    }

    private DataDirProvider dataDir(Path path) {
        return () -> path.toString();
    }

    private EmbeddingPort embedding() {
        return new EmbeddingPort() {
            @Override
            public float[] embed(String text) {
                if (text == null) {
                    return new float[]{0f, 0f};
                }
                String normalized = text.toLowerCase();
                if (normalized.contains("alpha")) {
                    return new float[]{1f, 0f};
                }
                if (normalized.contains("beta")) {
                    return new float[]{0f, 1f};
                }
                return new float[]{0.5f, 0.5f};
            }

            @Override
            public int dimension() {
                return 2;
            }
        };
    }
}
