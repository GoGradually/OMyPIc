package me.go_gradually.omypic.application.datatransfer.usecase;

import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.SerializationFeature;
import com.fasterxml.jackson.datatype.jsr310.JavaTimeModule;
import me.go_gradually.omypic.application.datatransfer.model.DataTransferImportResult;
import me.go_gradually.omypic.application.question.port.QuestionGroupPort;
import me.go_gradually.omypic.application.rulebook.model.StoredRulebookFile;
import me.go_gradually.omypic.application.rulebook.port.RulebookFileStore;
import me.go_gradually.omypic.application.rulebook.port.RulebookIndexPort;
import me.go_gradually.omypic.application.rulebook.port.RulebookPort;
import me.go_gradually.omypic.application.wrongnote.port.WrongNotePort;
import me.go_gradually.omypic.application.wrongnote.port.WrongNoteRecentQueuePort;
import me.go_gradually.omypic.domain.question.QuestionGroupAggregate;
import me.go_gradually.omypic.domain.question.QuestionGroupId;
import me.go_gradually.omypic.domain.question.QuestionItem;
import me.go_gradually.omypic.domain.question.QuestionItemId;
import me.go_gradually.omypic.domain.rulebook.Rulebook;
import me.go_gradually.omypic.domain.rulebook.RulebookId;
import me.go_gradually.omypic.domain.rulebook.RulebookScope;
import me.go_gradually.omypic.domain.wrongnote.WrongNote;
import me.go_gradually.omypic.domain.wrongnote.WrongNoteId;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.extension.ExtendWith;
import org.mockito.ArgumentCaptor;
import org.mockito.Mock;
import org.mockito.junit.jupiter.MockitoExtension;

import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.time.Instant;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.zip.ZipEntry;
import java.util.zip.ZipInputStream;
import java.util.zip.ZipOutputStream;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.mockito.ArgumentMatchers.any;
import static org.mockito.ArgumentMatchers.eq;
import static org.mockito.Mockito.verify;
import static org.mockito.Mockito.when;

@ExtendWith(MockitoExtension.class)
class DataTransferUseCaseTest {
    private static final Instant FIXED_TIME = Instant.parse("2026-02-19T00:00:00Z");

    @Mock
    private QuestionGroupPort questionGroupPort;
    @Mock
    private RulebookPort rulebookPort;
    @Mock
    private WrongNotePort wrongNotePort;
    @Mock
    private WrongNoteRecentQueuePort wrongNoteRecentQueuePort;
    @Mock
    private RulebookFileStore rulebookFileStore;
    @Mock
    private RulebookIndexPort rulebookIndexPort;

    private DataTransferUseCase useCase;
    private ObjectMapper objectMapper;

    @BeforeEach
    void setUp() {
        objectMapper = new ObjectMapper()
                .registerModule(new JavaTimeModule())
                .disable(SerializationFeature.WRITE_DATES_AS_TIMESTAMPS);
        useCase = new DataTransferUseCase(
                questionGroupPort,
                rulebookPort,
                wrongNotePort,
                wrongNoteRecentQueuePort,
                rulebookFileStore,
                rulebookIndexPort,
                objectMapper
        );
    }

    @Test
    void exportZip_includesRequiredEntries() throws Exception {
        QuestionGroupAggregate group = QuestionGroupAggregate.rehydrate(
                QuestionGroupId.of("g1"),
                "Travel",
                List.of("travel"),
                List.of(QuestionItem.rehydrate(QuestionItemId.of("q1"), "Q1", "habit")),
                FIXED_TIME,
                FIXED_TIME
        );
        Rulebook rulebook = Rulebook.rehydrate(
                RulebookId.of("r1"),
                "rules.md",
                "/tmp/rules.md",
                RulebookScope.MAIN,
                null,
                true,
                FIXED_TIME,
                FIXED_TIME
        );
        WrongNote wrongNote = WrongNote.rehydrate(
                WrongNoteId.of("w1"),
                "pattern",
                2,
                "summary",
                FIXED_TIME
        );
        when(questionGroupPort.findAll()).thenReturn(List.of(group));
        when(rulebookPort.findAll()).thenReturn(List.of(rulebook));
        when(wrongNotePort.findAll()).thenReturn(List.of(wrongNote));
        when(wrongNoteRecentQueuePort.loadGlobalQueue()).thenReturn(List.of("pattern"));
        when(rulebookFileStore.readBytes("/tmp/rules.md")).thenReturn("# rules".getBytes(StandardCharsets.UTF_8));

        byte[] exported = useCase.exportZip();
        Map<String, byte[]> zipEntries = unzipEntries(exported);

        assertEquals(true, zipEntries.containsKey("manifest.json"));
        assertEquals(true, zipEntries.containsKey("data/question-groups.json"));
        assertEquals(true, zipEntries.containsKey("data/rulebooks.json"));
        assertEquals(true, zipEntries.containsKey("data/wrong-notes.json"));
        assertEquals(true, zipEntries.containsKey("data/wrong-note-queue.json"));
        assertEquals(true, zipEntries.containsKey("files/rulebooks/r1_rules.md"));
        assertArrayEquals("# rules".getBytes(StandardCharsets.UTF_8), zipEntries.get("files/rulebooks/r1_rules.md"));
    }

    @Test
    void importZip_replacesAllDataAndReindexes() throws Exception {
        when(rulebookFileStore.store(eq("rules.md"), any(byte[].class))).thenReturn(new StoredRulebookFile("/tmp/new-rules.md"));
        when(rulebookFileStore.readText("/tmp/new-rules.md")).thenReturn("# Rules\nLine");

        byte[] importZip = createImportZip(1);

        DataTransferImportResult result = useCase.importZip(importZip);

        assertEquals(1, result.questionGroupCount());
        assertEquals(1, result.rulebookCount());
        assertEquals(1, result.wrongNoteCount());
        assertEquals(1, result.wrongNoteQueueSize());
        assertEquals(true, result.restartRequired());

        verify(questionGroupPort).deleteAll();
        verify(rulebookPort).deleteAll();
        verify(wrongNotePort).deleteAll();
        verify(wrongNoteRecentQueuePort).clearGlobalQueue();
        verify(rulebookFileStore).clearAll();
        verify(rulebookIndexPort).reset();
        verify(questionGroupPort).save(any(QuestionGroupAggregate.class));
        verify(rulebookPort).save(any(Rulebook.class));
        verify(wrongNotePort).save(any(WrongNote.class));
        verify(wrongNoteRecentQueuePort).saveGlobalQueue(List.of("pattern"));

        ArgumentCaptor<byte[]> fileCaptor = ArgumentCaptor.forClass(byte[].class);
        verify(rulebookFileStore).store(eq("rules.md"), fileCaptor.capture());
        assertArrayEquals("# rules".getBytes(StandardCharsets.UTF_8), fileCaptor.getValue());
    }

    @Test
    void importZip_rejectsUnsupportedSchemaVersion() throws Exception {
        byte[] importZip = createImportZip(2);

        assertThrows(IllegalArgumentException.class, () -> useCase.importZip(importZip));
    }

    private byte[] createImportZip(int schemaVersion) throws Exception {
        Map<String, byte[]> entries = new LinkedHashMap<>();
        entries.put("manifest.json", objectMapper.writeValueAsBytes(Map.of(
                "schemaVersion", schemaVersion,
                "app", "omypic",
                "exportedAt", FIXED_TIME
        )));
        entries.put("data/question-groups.json", objectMapper.writeValueAsBytes(List.of(Map.of(
                "id", "g1",
                "name", "Travel",
                "tags", List.of("travel"),
                "questions", List.of(Map.of("id", "q1", "text", "Q1", "questionType", "habit")),
                "createdAt", FIXED_TIME,
                "updatedAt", FIXED_TIME
        ))));
        Map<String, Object> rulebook = new LinkedHashMap<>();
        rulebook.put("id", "r1");
        rulebook.put("filename", "rules.md");
        rulebook.put("scope", "MAIN");
        rulebook.put("questionGroup", null);
        rulebook.put("enabled", true);
        rulebook.put("createdAt", FIXED_TIME);
        rulebook.put("updatedAt", FIXED_TIME);
        rulebook.put("fileEntry", "files/rulebooks/r1_rules.md");
        entries.put("data/rulebooks.json", objectMapper.writeValueAsBytes(List.of(rulebook)));
        entries.put("data/wrong-notes.json", objectMapper.writeValueAsBytes(List.of(Map.of(
                "id", "w1",
                "pattern", "pattern",
                "count", 1,
                "shortSummary", "summary",
                "lastSeenAt", FIXED_TIME
        ))));
        entries.put("data/wrong-note-queue.json", objectMapper.writeValueAsBytes(Map.of(
                "patterns", List.of("pattern")
        )));
        entries.put("files/rulebooks/r1_rules.md", "# rules".getBytes(StandardCharsets.UTF_8));
        return toZip(entries);
    }

    private byte[] toZip(Map<String, byte[]> entries) throws IOException {
        try (ByteArrayOutputStream out = new ByteArrayOutputStream();
             ZipOutputStream zip = new ZipOutputStream(out)) {
            for (Map.Entry<String, byte[]> entry : entries.entrySet()) {
                zip.putNextEntry(new ZipEntry(entry.getKey()));
                zip.write(entry.getValue());
                zip.closeEntry();
            }
            zip.finish();
            return out.toByteArray();
        }
    }

    private Map<String, byte[]> unzipEntries(byte[] zipped) throws IOException {
        Map<String, byte[]> entries = new LinkedHashMap<>();
        try (ZipInputStream zip = new ZipInputStream(new ByteArrayInputStream(zipped))) {
            ZipEntry entry;
            while ((entry = zip.getNextEntry()) != null) {
                if (!entry.isDirectory()) {
                    entries.put(entry.getName(), zip.readAllBytes());
                }
                zip.closeEntry();
            }
        }
        return entries;
    }
}
