package me.go_gradually.omypic.domain.rulebook;

public record RulebookContext(RulebookId rulebookId, String filename, String text) {
    public RulebookContext(RulebookId rulebookId, String filename, String text) {
        if (rulebookId == null) {
            throw new IllegalArgumentException("RulebookId is required");
        }
        this.rulebookId = rulebookId;
        this.filename = filename == null ? "" : filename;
        this.text = text == null ? "" : text;
    }

    public static RulebookContext of(RulebookId rulebookId, String filename, String text) {
        return new RulebookContext(rulebookId, filename, text);
    }
}
