package me.go_gradually.omypic.domain.rulebook;

import java.time.Instant;

public class Rulebook {
    private final RulebookId id;
    private final String filename;
    private final String path;
    private boolean enabled;
    private final Instant createdAt;
    private Instant updatedAt;

    private Rulebook(RulebookId id, String filename, String path, boolean enabled, Instant createdAt, Instant updatedAt) {
        if (id == null) {
            throw new IllegalArgumentException("Rulebook id is required");
        }
        this.id = id;
        this.filename = filename == null ? "" : filename;
        this.path = path == null ? "" : path;
        this.enabled = enabled;
        this.createdAt = createdAt == null ? Instant.now() : createdAt;
        this.updatedAt = updatedAt == null ? this.createdAt : updatedAt;
    }

    public static Rulebook create(String filename, String path, Instant now) {
        Instant timestamp = now == null ? Instant.now() : now;
        return new Rulebook(RulebookId.newId(), filename, path, true, timestamp, timestamp);
    }

    public static Rulebook rehydrate(RulebookId id, String filename, String path, boolean enabled, Instant createdAt, Instant updatedAt) {
        return new Rulebook(id, filename, path, enabled, createdAt, updatedAt);
    }

    public void toggle(boolean enabled, Instant now) {
        this.enabled = enabled;
        touchUpdatedAt(now);
    }

    private void touchUpdatedAt(Instant now) {
        this.updatedAt = now == null ? Instant.now() : now;
    }

    public RulebookId getId() {
        return id;
    }

    public String getFilename() {
        return filename;
    }

    public String getPath() {
        return path;
    }

    public boolean isEnabled() {
        return enabled;
    }

    public Instant getCreatedAt() {
        return createdAt;
    }

    public Instant getUpdatedAt() {
        return updatedAt;
    }
}
