package me.go_gradually.omypic.domain.rulebook;

import java.time.Instant;

public class Rulebook {
    private final RulebookId id;
    private final String filename;
    private final String path;
    private final RulebookScope scope;
    private final String questionGroup;
    private boolean enabled;
    private final Instant createdAt;
    private Instant updatedAt;

    private Rulebook(RulebookId id,
                     String filename,
                     String path,
                     RulebookScope scope,
                     String questionGroup,
                     boolean enabled,
                     Instant createdAt,
                     Instant updatedAt) {
        if (id == null) {
            throw new IllegalArgumentException("Rulebook id is required");
        }
        this.id = id;
        this.filename = filename == null ? "" : filename;
        this.path = path == null ? "" : path;
        this.scope = scope == null ? RulebookScope.MAIN : scope;
        this.questionGroup = questionGroup == null ? "" : questionGroup.trim();
        this.enabled = enabled;
        this.createdAt = createdAt == null ? Instant.now() : createdAt;
        this.updatedAt = updatedAt == null ? this.createdAt : updatedAt;
    }

    public static Rulebook create(String filename, String path, RulebookScope scope, String questionGroup, Instant now) {
        Instant timestamp = now == null ? Instant.now() : now;
        return new Rulebook(RulebookId.newId(), filename, path, scope, questionGroup, true, timestamp, timestamp);
    }

    public static Rulebook rehydrate(RulebookId id,
                                     String filename,
                                     String path,
                                     RulebookScope scope,
                                     String questionGroup,
                                     boolean enabled,
                                     Instant createdAt,
                                     Instant updatedAt) {
        return new Rulebook(id, filename, path, scope, questionGroup, enabled, createdAt, updatedAt);
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

    public RulebookScope getScope() {
        return scope;
    }

    public String getQuestionGroup() {
        return questionGroup;
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
