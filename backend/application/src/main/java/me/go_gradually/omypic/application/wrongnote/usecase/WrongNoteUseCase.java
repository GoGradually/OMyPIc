package me.go_gradually.omypic.application.wrongnote.usecase;

import me.go_gradually.omypic.application.feedback.policy.FeedbackPolicy;
import me.go_gradually.omypic.application.wrongnote.port.WrongNotePort;
import me.go_gradually.omypic.application.wrongnote.port.WrongNoteRecentQueuePort;
import me.go_gradually.omypic.domain.feedback.CorrectionDetail;
import me.go_gradually.omypic.domain.feedback.RecommendationDetail;
import me.go_gradually.omypic.domain.feedback.Feedback;
import me.go_gradually.omypic.domain.shared.util.TextUtils;
import me.go_gradually.omypic.domain.wrongnote.WrongNote;
import me.go_gradually.omypic.domain.wrongnote.WrongNoteWindow;

import java.time.Instant;
import java.util.Comparator;
import java.util.List;

public class WrongNoteUseCase {
    private final WrongNotePort repository;
    private final WrongNoteRecentQueuePort recentQueueStore;
    private final FeedbackPolicy feedbackPolicy;

    public WrongNoteUseCase(WrongNotePort repository,
                            WrongNoteRecentQueuePort recentQueueStore,
                            FeedbackPolicy feedbackPolicy) {
        this.repository = repository;
        this.recentQueueStore = recentQueueStore;
        this.feedbackPolicy = feedbackPolicy;
    }

    public synchronized void addFeedback(Feedback response) {
        WrongNoteWindow window = loadWindow();
        applyIfPresent(window, correctionPoint("Grammar", response.getCorrections().grammar()));
        applyIfPresent(window, correctionPoint("Expression", response.getCorrections().expression()));
        applyIfPresent(window, correctionPoint("Logic", response.getCorrections().logic()));
        applyIfPresent(window, recommendationPoint("Filler", response.getRecommendations().filler()));
        applyIfPresent(window, recommendationPoint("Adjective", response.getRecommendations().adjective()));
        applyIfPresent(window, recommendationPoint("Adverb", response.getRecommendations().adverb()));
        recentQueueStore.saveGlobalQueue(window.snapshot());
    }

    private void applyIfPresent(WrongNoteWindow window, String point) {
        if (point == null || point.isBlank()) {
            return;
        }
        applyCorrectionPoint(window, point);
    }

    private String correctionPoint(String category, CorrectionDetail detail) {
        String issue = detail == null ? "" : detail.issue();
        String fix = detail == null ? "" : detail.fix();
        if (!issue.isBlank() && !fix.isBlank()) {
            return category + ": " + issue + " | " + fix;
        }
        if (!issue.isBlank()) {
            return category + ": " + issue;
        }
        if (!fix.isBlank()) {
            return category + ": " + fix;
        }
        return "";
    }

    private String recommendationPoint(String category, RecommendationDetail detail) {
        String term = detail == null ? "" : detail.term();
        String usage = detail == null ? "" : detail.usage();
        if (!term.isBlank() && !usage.isBlank()) {
            return category + ": " + term + " - " + usage;
        }
        if (!term.isBlank()) {
            return category + ": " + term;
        }
        if (!usage.isBlank()) {
            return category + ": " + usage;
        }
        return "";
    }

    private WrongNoteWindow loadWindow() {
        return WrongNoteWindow.from(
                recentQueueStore.loadGlobalQueue(),
                feedbackPolicy.getWrongnoteWindowSize()
        );
    }

    private void applyCorrectionPoint(WrongNoteWindow window, String point) {
        String pattern = TextUtils.trimToLength(point, 120);
        WrongNote note = findOrCreate(pattern);
        note.recordOccurrence(point, feedbackPolicy.getWrongnoteSummaryMaxChars(), Instant.now());
        repository.save(note);
        removeEvictedPattern(window.append(pattern));
    }

    private WrongNote findOrCreate(String pattern) {
        return repository.findByPattern(pattern).orElseGet(() -> WrongNote.createNew(pattern, Instant.now()));
    }

    private void removeEvictedPattern(String pattern) {
        if (pattern != null) {
            decrementOrRemove(pattern);
        }
    }

    private void decrementOrRemove(String pattern) {
        WrongNote note = repository.findByPattern(pattern).orElse(null);
        if (note == null) {
            return;
        }
        boolean shouldDelete = note.decrement();
        if (shouldDelete) {
            repository.deleteById(note.getId());
        } else {
            repository.save(note);
        }
    }

    public List<WrongNote> list() {
        List<WrongNote> docs = repository.findAll();
        docs.sort(Comparator.comparingInt(WrongNote::getCount).reversed());
        return docs;
    }
}
