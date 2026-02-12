package me.go_gradually.omypic.domain.rulebook;

import me.go_gradually.omypic.domain.question.QuestionGroup;

import java.time.Instant;

public class Rulebook {
    private final RulebookId id;
    private final String filename;
    private final String path;
    private final RulebookScope scope;
    private final QuestionGroup questionGroup;
    private boolean enabled;
    private final Instant createdAt;
    private Instant updatedAt;

    private Rulebook(RulebookId id,
                     String filename,
                     String path,
                     RulebookScope scope,
                     QuestionGroup questionGroup,
                     boolean enabled,
                     Instant createdAt,
                     Instant updatedAt) {
        if (id == null) {
            throw new IllegalArgumentException("Rulebook id is required");
        }
        this.id = id;
        this.filename = filename == null ? "" : filename;
        this.path = path == null ? "" : path;
        RulebookScope resolvedScope = scope == null ? RulebookScope.MAIN : scope;
        validateScope(resolvedScope, questionGroup);
        this.scope = resolvedScope;
        this.questionGroup = questionGroup;
        this.enabled = enabled;
        this.createdAt = createdAt == null ? Instant.now() : createdAt;
        this.updatedAt = updatedAt == null ? this.createdAt : updatedAt;
    }

    public static Rulebook create(String filename, String path, RulebookScope scope, QuestionGroup questionGroup, Instant now) {
        Instant timestamp = now == null ? Instant.now() : now;
        return new Rulebook(RulebookId.newId(), filename, path, scope, questionGroup, true, timestamp, timestamp);
    }

    public static Rulebook rehydrate(RulebookId id,
                                     String filename,
                                     String path,
                                     RulebookScope scope,
                                     QuestionGroup questionGroup,
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

    private static void validateScope(RulebookScope scope, QuestionGroup questionGroup) {
        if (scope == RulebookScope.MAIN && questionGroup != null) {
            throw new IllegalArgumentException("MAIN scope does not allow questionGroup");
        }
        if (scope == RulebookScope.QUESTION && questionGroup == null) {
            throw new IllegalArgumentException("QUESTION scope requires questionGroup");
        }
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

    public QuestionGroup getQuestionGroup() {
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
