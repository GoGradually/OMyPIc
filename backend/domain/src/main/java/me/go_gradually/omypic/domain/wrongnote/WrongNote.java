package me.go_gradually.omypic.domain.wrongnote;

import me.go_gradually.omypic.domain.shared.util.TextUtils;

import java.time.Instant;

public class WrongNote {
    private final WrongNoteId id;
    private final String pattern;
    private int count;
    private String shortSummary;
    private Instant lastSeenAt;

    private WrongNote(WrongNoteId id, String pattern, int count, String shortSummary, Instant lastSeenAt) {
        if (id == null) {
            throw new IllegalArgumentException("WrongNote id is required");
        }
        this.id = id;
        this.pattern = pattern == null ? "" : pattern;
        this.count = count;
        this.shortSummary = shortSummary == null ? "" : shortSummary;
        this.lastSeenAt = lastSeenAt == null ? Instant.now() : lastSeenAt;
    }

    public static WrongNote createNew(String pattern, Instant now) {
        return new WrongNote(WrongNoteId.newId(), pattern, 0, "", now);
    }

    public static WrongNote rehydrate(WrongNoteId id, String pattern, int count, String shortSummary, Instant lastSeenAt) {
        return new WrongNote(id, pattern, count, shortSummary, lastSeenAt);
    }

    public void recordOccurrence(String summary, int maxSummaryChars, Instant now) {
        count += 1;
        shortSummary = TextUtils.trimToLength(summary, maxSummaryChars);
        lastSeenAt = now == null ? Instant.now() : now;
    }

    public boolean decrement() {
        count -= 1;
        return count <= 0;
    }

    public WrongNoteId getId() {
        return id;
    }

    public String getPattern() {
        return pattern;
    }

    public int getCount() {
        return count;
    }

    public String getShortSummary() {
        return shortSummary;
    }

    public Instant getLastSeenAt() {
        return lastSeenAt;
    }
}
