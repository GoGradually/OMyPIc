package me.go_gradually.omypic.application.datatransfer.usecase;

import com.fasterxml.jackson.core.type.TypeReference;
import com.fasterxml.jackson.databind.ObjectMapper;
import me.go_gradually.omypic.application.datatransfer.model.DataTransferImportResult;
import me.go_gradually.omypic.application.question.port.QuestionGroupPort;
import me.go_gradually.omypic.application.rulebook.port.RulebookFileStore;
import me.go_gradually.omypic.application.rulebook.port.RulebookIndexPort;
import me.go_gradually.omypic.application.rulebook.port.RulebookPort;
import me.go_gradually.omypic.application.wrongnote.port.WrongNotePort;
import me.go_gradually.omypic.application.wrongnote.port.WrongNoteRecentQueuePort;
import me.go_gradually.omypic.domain.question.QuestionGroupAggregate;
import me.go_gradually.omypic.domain.question.QuestionGroupId;
import me.go_gradually.omypic.domain.question.QuestionItem;
import me.go_gradually.omypic.domain.question.QuestionItemId;
import me.go_gradually.omypic.domain.question.QuestionGroup;
import me.go_gradually.omypic.domain.rulebook.Rulebook;
import me.go_gradually.omypic.domain.rulebook.RulebookId;
import me.go_gradually.omypic.domain.rulebook.RulebookScope;
import me.go_gradually.omypic.domain.shared.util.TextUtils;
import me.go_gradually.omypic.domain.wrongnote.WrongNote;
import me.go_gradually.omypic.domain.wrongnote.WrongNoteId;

import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.time.Instant;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.regex.Pattern;
import java.util.zip.ZipEntry;
import java.util.zip.ZipInputStream;
import java.util.zip.ZipOutputStream;

public class DataTransferUseCase {
    private static final int SCHEMA_VERSION = 1;
    private static final String MANIFEST_PATH = "manifest.json";
    private static final String QUESTION_GROUPS_PATH = "data/question-groups.json";
    private static final String RULEBOOKS_PATH = "data/rulebooks.json";
    private static final String WRONG_NOTES_PATH = "data/wrong-notes.json";
    private static final String WRONG_NOTE_QUEUE_PATH = "data/wrong-note-queue.json";
    private static final String RULEBOOK_FILE_PREFIX = "files/rulebooks/";
    private static final Pattern SAFE_FILENAME = Pattern.compile("[^a-zA-Z0-9._-]");
    private static final TypeReference<List<QuestionGroupSnapshot>> QUESTION_GROUP_LIST_TYPE = new TypeReference<>() {
    };
    private static final TypeReference<List<RulebookSnapshot>> RULEBOOK_LIST_TYPE = new TypeReference<>() {
    };
    private static final TypeReference<List<WrongNoteSnapshot>> WRONG_NOTE_LIST_TYPE = new TypeReference<>() {
    };
    private static final TypeReference<WrongNoteQueueSnapshot> WRONG_NOTE_QUEUE_TYPE = new TypeReference<>() {
    };

    private final QuestionGroupPort questionGroupPort;
    private final RulebookPort rulebookPort;
    private final WrongNotePort wrongNotePort;
    private final WrongNoteRecentQueuePort wrongNoteRecentQueuePort;
    private final RulebookFileStore rulebookFileStore;
    private final RulebookIndexPort rulebookIndexPort;
    private final ObjectMapper objectMapper;

    public DataTransferUseCase(QuestionGroupPort questionGroupPort,
                               RulebookPort rulebookPort,
                               WrongNotePort wrongNotePort,
                               WrongNoteRecentQueuePort wrongNoteRecentQueuePort,
                               RulebookFileStore rulebookFileStore,
                               RulebookIndexPort rulebookIndexPort,
                               ObjectMapper objectMapper) {
        this.questionGroupPort = questionGroupPort;
        this.rulebookPort = rulebookPort;
        this.wrongNotePort = wrongNotePort;
        this.wrongNoteRecentQueuePort = wrongNoteRecentQueuePort;
        this.rulebookFileStore = rulebookFileStore;
        this.rulebookIndexPort = rulebookIndexPort;
        this.objectMapper = objectMapper;
    }

    public byte[] exportZip() throws IOException {
        List<QuestionGroupSnapshot> questionGroups = questionGroupPort.findAll().stream()
                .map(QuestionGroupSnapshot::fromDomain)
                .toList();
        List<RulebookFileSnapshot> rulebookFiles = new ArrayList<>();
        List<RulebookSnapshot> rulebooks = collectRulebookSnapshots(rulebookFiles);
        List<WrongNoteSnapshot> wrongNotes = wrongNotePort.findAll().stream()
                .map(WrongNoteSnapshot::fromDomain)
                .toList();
        WrongNoteQueueSnapshot queue = new WrongNoteQueueSnapshot();
        queue.patterns = wrongNoteRecentQueuePort.loadGlobalQueue();

        BackupManifest manifest = new BackupManifest();
        manifest.schemaVersion = SCHEMA_VERSION;
        manifest.app = "omypic";
        manifest.exportedAt = Instant.now();

        try (ByteArrayOutputStream bytes = new ByteArrayOutputStream();
             ZipOutputStream zip = new ZipOutputStream(bytes)) {
            writeJson(zip, MANIFEST_PATH, manifest);
            writeJson(zip, QUESTION_GROUPS_PATH, questionGroups);
            writeJson(zip, RULEBOOKS_PATH, rulebooks);
            writeJson(zip, WRONG_NOTES_PATH, wrongNotes);
            writeJson(zip, WRONG_NOTE_QUEUE_PATH, queue);
            for (RulebookFileSnapshot rulebookFile : rulebookFiles) {
                writeBytes(zip, rulebookFile.entryPath, rulebookFile.bytes);
            }
            zip.finish();
            return bytes.toByteArray();
        }
    }

    public DataTransferImportResult importZip(byte[] zipBytes) throws IOException {
        if (zipBytes == null || zipBytes.length == 0) {
            throw new IllegalArgumentException("Import file is empty");
        }
        Map<String, byte[]> entries = readZipEntries(zipBytes);
        ensureRequiredEntries(entries);

        BackupManifest manifest = readJson(entries.get(MANIFEST_PATH), BackupManifest.class);
        if (manifest.schemaVersion != SCHEMA_VERSION) {
            throw new IllegalArgumentException("Unsupported backup schema version: " + manifest.schemaVersion);
        }

        List<QuestionGroupSnapshot> questionGroups = readJson(entries.get(QUESTION_GROUPS_PATH), QUESTION_GROUP_LIST_TYPE);
        List<RulebookSnapshot> rulebooks = readJson(entries.get(RULEBOOKS_PATH), RULEBOOK_LIST_TYPE);
        List<WrongNoteSnapshot> wrongNotes = readJson(entries.get(WRONG_NOTES_PATH), WRONG_NOTE_LIST_TYPE);
        WrongNoteQueueSnapshot queueSnapshot = readJson(entries.get(WRONG_NOTE_QUEUE_PATH), WRONG_NOTE_QUEUE_TYPE);
        if (questionGroups == null || rulebooks == null || wrongNotes == null || queueSnapshot == null) {
            throw new IllegalArgumentException("Backup payload is invalid");
        }
        validateRulebookEntries(rulebooks, entries);

        ImportedPayload payload = toImportedPayload(questionGroups, rulebooks, wrongNotes, queueSnapshot, entries);
        applyImportedPayload(payload);

        return new DataTransferImportResult(
                Instant.now(),
                payload.questionGroups.size(),
                payload.rulebooks.size(),
                payload.wrongNotes.size(),
                payload.wrongNoteQueue.size(),
                true
        );
    }

    private void ensureRequiredEntries(Map<String, byte[]> entries) {
        requireEntry(entries, MANIFEST_PATH);
        requireEntry(entries, QUESTION_GROUPS_PATH);
        requireEntry(entries, RULEBOOKS_PATH);
        requireEntry(entries, WRONG_NOTES_PATH);
        requireEntry(entries, WRONG_NOTE_QUEUE_PATH);
    }

    private void requireEntry(Map<String, byte[]> entries, String path) {
        if (!entries.containsKey(path)) {
            throw new IllegalArgumentException("Required entry is missing: " + path);
        }
    }

    private List<RulebookSnapshot> collectRulebookSnapshots(List<RulebookFileSnapshot> rulebookFiles) throws IOException {
        List<RulebookSnapshot> snapshots = new ArrayList<>();
        for (Rulebook rulebook : rulebookPort.findAll()) {
            String entryPath = toRulebookEntryPath(rulebook.getId().value(), rulebook.getFilename());
            byte[] bytes = rulebookFileStore.readBytes(rulebook.getPath());
            rulebookFiles.add(new RulebookFileSnapshot(entryPath, bytes));
            snapshots.add(RulebookSnapshot.fromDomain(rulebook, entryPath));
        }
        return snapshots;
    }

    private String toRulebookEntryPath(String rulebookId, String filename) {
        String safeName = filename == null ? "rulebook.md" : SAFE_FILENAME.matcher(filename).replaceAll("_");
        if (safeName.isBlank()) {
            safeName = "rulebook.md";
        }
        return RULEBOOK_FILE_PREFIX + rulebookId + "_" + safeName;
    }

    private void writeJson(ZipOutputStream zip, String path, Object value) throws IOException {
        writeBytes(zip, path, objectMapper.writerWithDefaultPrettyPrinter().writeValueAsBytes(value));
    }

    private void writeBytes(ZipOutputStream zip, String path, byte[] bytes) throws IOException {
        ZipEntry entry = new ZipEntry(path);
        zip.putNextEntry(entry);
        zip.write(bytes);
        zip.closeEntry();
    }

    private Map<String, byte[]> readZipEntries(byte[] zipBytes) throws IOException {
        Map<String, byte[]> entries = new LinkedHashMap<>();
        try (ZipInputStream zip = new ZipInputStream(new ByteArrayInputStream(zipBytes))) {
            ZipEntry entry;
            while ((entry = zip.getNextEntry()) != null) {
                if (!entry.isDirectory()) {
                    validateEntryName(entry.getName());
                    byte[] previous = entries.putIfAbsent(entry.getName(), zip.readAllBytes());
                    if (previous != null) {
                        throw new IllegalArgumentException("Duplicated entry in backup file: " + entry.getName());
                    }
                }
                zip.closeEntry();
            }
        }
        return entries;
    }

    private void validateEntryName(String name) {
        if (name == null || name.isBlank()) {
            throw new IllegalArgumentException("Backup entry name is invalid");
        }
        if (name.startsWith("/") || name.contains("..")) {
            throw new IllegalArgumentException("Backup entry name is unsafe: " + name);
        }
    }

    private void validateRulebookEntries(List<RulebookSnapshot> rulebooks, Map<String, byte[]> entries) {
        for (RulebookSnapshot rulebook : rulebooks) {
            if (rulebook.fileEntry == null || rulebook.fileEntry.isBlank()) {
                throw new IllegalArgumentException("Rulebook file entry is missing");
            }
            validateEntryName(rulebook.fileEntry);
            if (!rulebook.fileEntry.startsWith(RULEBOOK_FILE_PREFIX)) {
                throw new IllegalArgumentException("Rulebook file entry is outside allowed path: " + rulebook.fileEntry);
            }
            if (!entries.containsKey(rulebook.fileEntry)) {
                throw new IllegalArgumentException("Rulebook file is missing from backup: " + rulebook.fileEntry);
            }
        }
    }

    private ImportedPayload toImportedPayload(List<QuestionGroupSnapshot> questionGroups,
                                              List<RulebookSnapshot> rulebooks,
                                              List<WrongNoteSnapshot> wrongNotes,
                                              WrongNoteQueueSnapshot queueSnapshot,
                                              Map<String, byte[]> entries) {
        List<QuestionGroupAggregate> importedQuestionGroups = questionGroups.stream()
                .map(QuestionGroupSnapshot::toDomain)
                .toList();
        List<ImportedRulebook> importedRulebooks = rulebooks.stream()
                .map(snapshot -> new ImportedRulebook(snapshot, entries.get(snapshot.fileEntry)))
                .toList();
        List<WrongNote> importedWrongNotes = wrongNotes.stream()
                .map(WrongNoteSnapshot::toDomain)
                .toList();
        List<String> queue = queueSnapshot == null || queueSnapshot.patterns == null
                ? List.of()
                : List.copyOf(queueSnapshot.patterns);
        return new ImportedPayload(importedQuestionGroups, importedRulebooks, importedWrongNotes, queue);
    }

    private void applyImportedPayload(ImportedPayload payload) throws IOException {
        questionGroupPort.deleteAll();
        rulebookPort.deleteAll();
        wrongNotePort.deleteAll();
        wrongNoteRecentQueuePort.clearGlobalQueue();
        rulebookFileStore.clearAll();
        rulebookIndexPort.reset();

        for (QuestionGroupAggregate group : payload.questionGroups) {
            questionGroupPort.save(group);
        }
        for (ImportedRulebook importedRulebook : payload.rulebooks) {
            RulebookSnapshot snapshot = importedRulebook.snapshot;
            var stored = rulebookFileStore.store(snapshot.filename, importedRulebook.bytes);
            Rulebook rulebook = Rulebook.rehydrate(
                    RulebookId.of(snapshot.id),
                    snapshot.filename,
                    stored.path(),
                    snapshot.scope,
                    QuestionGroup.fromNullable(snapshot.questionGroup),
                    snapshot.enabled,
                    snapshot.createdAt,
                    snapshot.updatedAt
            );
            rulebookPort.save(rulebook);
            String text = rulebookFileStore.readText(stored.path());
            List<String> chunks = TextUtils.splitChunks(text, 800);
            rulebookIndexPort.indexRulebookChunks(rulebook.getId(), rulebook.getFilename(), chunks);
        }
        for (WrongNote wrongNote : payload.wrongNotes) {
            wrongNotePort.save(wrongNote);
        }
        wrongNoteRecentQueuePort.saveGlobalQueue(payload.wrongNoteQueue);
    }

    private <T> T readJson(byte[] bytes, Class<T> type) throws IOException {
        return objectMapper.readValue(bytes, type);
    }

    private <T> T readJson(byte[] bytes, TypeReference<T> type) throws IOException {
        return objectMapper.readValue(bytes, type);
    }

    private record ImportedPayload(
            List<QuestionGroupAggregate> questionGroups,
            List<ImportedRulebook> rulebooks,
            List<WrongNote> wrongNotes,
            List<String> wrongNoteQueue
    ) {
    }

    private record ImportedRulebook(RulebookSnapshot snapshot, byte[] bytes) {
    }

    private record RulebookFileSnapshot(String entryPath, byte[] bytes) {
    }

    private static class BackupManifest {
        public int schemaVersion;
        public String app;
        public Instant exportedAt;
    }

    private static class WrongNoteQueueSnapshot {
        public List<String> patterns = List.of();
    }

    private static class QuestionGroupSnapshot {
        public String id;
        public String name;
        public List<String> tags = List.of();
        public List<QuestionItemSnapshot> questions = List.of();
        public Instant createdAt;
        public Instant updatedAt;

        static QuestionGroupSnapshot fromDomain(QuestionGroupAggregate group) {
            QuestionGroupSnapshot snapshot = new QuestionGroupSnapshot();
            snapshot.id = group.getId().value();
            snapshot.name = group.getName();
            snapshot.tags = group.getTags().stream().sorted().toList();
            snapshot.questions = group.getQuestions().stream()
                    .map(QuestionItemSnapshot::fromDomain)
                    .toList();
            snapshot.createdAt = group.getCreatedAt();
            snapshot.updatedAt = group.getUpdatedAt();
            return snapshot;
        }

        QuestionGroupAggregate toDomain() {
            List<QuestionItem> items = questions == null ? List.of() : questions.stream()
                    .map(QuestionItemSnapshot::toDomain)
                    .toList();
            return QuestionGroupAggregate.rehydrate(
                    QuestionGroupId.of(id),
                    name,
                    tags,
                    items,
                    createdAt,
                    updatedAt
            );
        }
    }

    private static class QuestionItemSnapshot {
        public String id;
        public String text;
        public String questionType;

        static QuestionItemSnapshot fromDomain(QuestionItem item) {
            QuestionItemSnapshot snapshot = new QuestionItemSnapshot();
            snapshot.id = item.getId().value();
            snapshot.text = item.getText();
            snapshot.questionType = item.getQuestionType();
            return snapshot;
        }

        QuestionItem toDomain() {
            return QuestionItem.rehydrate(QuestionItemId.of(id), text, questionType);
        }
    }

    private static class RulebookSnapshot {
        public String id;
        public String filename;
        public RulebookScope scope;
        public String questionGroup;
        public boolean enabled;
        public Instant createdAt;
        public Instant updatedAt;
        public String fileEntry;

        static RulebookSnapshot fromDomain(Rulebook rulebook, String fileEntry) {
            RulebookSnapshot snapshot = new RulebookSnapshot();
            snapshot.id = rulebook.getId().value();
            snapshot.filename = rulebook.getFilename();
            snapshot.scope = rulebook.getScope();
            snapshot.questionGroup = rulebook.getQuestionGroup() == null ? null : rulebook.getQuestionGroup().value();
            snapshot.enabled = rulebook.isEnabled();
            snapshot.createdAt = rulebook.getCreatedAt();
            snapshot.updatedAt = rulebook.getUpdatedAt();
            snapshot.fileEntry = fileEntry;
            return snapshot;
        }
    }

    private static class WrongNoteSnapshot {
        public String id;
        public String pattern;
        public int count;
        public String shortSummary;
        public Instant lastSeenAt;

        static WrongNoteSnapshot fromDomain(WrongNote wrongNote) {
            WrongNoteSnapshot snapshot = new WrongNoteSnapshot();
            snapshot.id = wrongNote.getId().value();
            snapshot.pattern = wrongNote.getPattern();
            snapshot.count = wrongNote.getCount();
            snapshot.shortSummary = wrongNote.getShortSummary();
            snapshot.lastSeenAt = wrongNote.getLastSeenAt();
            return snapshot;
        }

        WrongNote toDomain() {
            return WrongNote.rehydrate(
                    WrongNoteId.of(id),
                    pattern,
                    count,
                    shortSummary,
                    lastSeenAt
            );
        }
    }
}
