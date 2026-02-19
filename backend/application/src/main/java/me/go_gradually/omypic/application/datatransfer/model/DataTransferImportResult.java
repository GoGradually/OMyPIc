package me.go_gradually.omypic.application.datatransfer.model;

import java.time.Instant;

public record DataTransferImportResult(
        Instant importedAt,
        int questionGroupCount,
        int rulebookCount,
        int wrongNoteCount,
        int wrongNoteQueueSize,
        boolean restartRequired
) {
}
