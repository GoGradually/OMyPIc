package me.go_gradually.omypic.presentation.datatransfer.dto;

import java.time.Instant;

public class DataTransferImportResponse {
    private Instant importedAt;
    private int questionGroupCount;
    private int rulebookCount;
    private int wrongNoteCount;
    private int wrongNoteQueueSize;
    private boolean restartRequired;

    public Instant getImportedAt() {
        return importedAt;
    }

    public void setImportedAt(Instant importedAt) {
        this.importedAt = importedAt;
    }

    public int getQuestionGroupCount() {
        return questionGroupCount;
    }

    public void setQuestionGroupCount(int questionGroupCount) {
        this.questionGroupCount = questionGroupCount;
    }

    public int getRulebookCount() {
        return rulebookCount;
    }

    public void setRulebookCount(int rulebookCount) {
        this.rulebookCount = rulebookCount;
    }

    public int getWrongNoteCount() {
        return wrongNoteCount;
    }

    public void setWrongNoteCount(int wrongNoteCount) {
        this.wrongNoteCount = wrongNoteCount;
    }

    public int getWrongNoteQueueSize() {
        return wrongNoteQueueSize;
    }

    public void setWrongNoteQueueSize(int wrongNoteQueueSize) {
        this.wrongNoteQueueSize = wrongNoteQueueSize;
    }

    public boolean isRestartRequired() {
        return restartRequired;
    }

    public void setRestartRequired(boolean restartRequired) {
        this.restartRequired = restartRequired;
    }
}
