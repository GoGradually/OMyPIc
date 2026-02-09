package me.go_gradually.omypic.application.wrongnote.usecase;

import me.go_gradually.omypic.application.feedback.policy.FeedbackPolicy;
import me.go_gradually.omypic.application.wrongnote.port.WrongNotePort;
import me.go_gradually.omypic.application.wrongnote.port.WrongNoteRecentQueuePort;
import me.go_gradually.omypic.domain.feedback.Feedback;
import me.go_gradually.omypic.domain.shared.util.TextUtils;
import me.go_gradually.omypic.domain.wrongnote.WrongNote;

import java.time.Instant;
import java.util.ArrayDeque;
import java.util.Comparator;
import java.util.Deque;
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
        Deque<String> recentQueue = new ArrayDeque<>(recentQueueStore.loadGlobalQueue());
        for (String point : response.getCorrectionPoints()) {
            String pattern = TextUtils.trimToLength(point, 120);
            WrongNote note = repository.findByPattern(pattern)
                    .orElseGet(() -> WrongNote.createNew(pattern, Instant.now()));
            note.recordOccurrence(point, feedbackPolicy.getWrongnoteSummaryMaxChars(), Instant.now());
            repository.save(note);
            recentQueue.addLast(pattern);
            if (recentQueue.size() > 30) {
                String removed = recentQueue.removeFirst();
                decrementOrRemove(removed);
            }
        }
        recentQueueStore.saveGlobalQueue(List.copyOf(recentQueue));
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
