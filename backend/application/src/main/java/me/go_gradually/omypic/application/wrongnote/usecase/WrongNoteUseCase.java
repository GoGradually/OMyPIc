package me.go_gradually.omypic.application.wrongnote.usecase;

import me.go_gradually.omypic.application.feedback.policy.FeedbackPolicy;
import me.go_gradually.omypic.application.wrongnote.port.WrongNotePort;
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
    private final FeedbackPolicy feedbackPolicy;
    private final Deque<String> recentQueue = new ArrayDeque<>();

    public WrongNoteUseCase(WrongNotePort repository, FeedbackPolicy feedbackPolicy) {
        this.repository = repository;
        this.feedbackPolicy = feedbackPolicy;
    }

    public void addFeedback(Feedback response) {
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
