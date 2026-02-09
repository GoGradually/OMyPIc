package me.go_gradually.omypic.application.wrongnote.usecase;

import me.go_gradually.omypic.application.feedback.policy.FeedbackPolicy;
import me.go_gradually.omypic.application.wrongnote.port.WrongNotePort;
import me.go_gradually.omypic.domain.feedback.Feedback;
import me.go_gradually.omypic.domain.wrongnote.WrongNote;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.extension.ExtendWith;
import org.mockito.Mock;
import org.mockito.junit.jupiter.MockitoExtension;

import java.util.*;

import static org.junit.jupiter.api.Assertions.*;
import static org.mockito.ArgumentMatchers.any;
import static org.mockito.ArgumentMatchers.anyString;
import static org.mockito.Mockito.lenient;

@ExtendWith(MockitoExtension.class)
class WrongNoteUseCaseTest {

    private final Map<String, WrongNote> storage = new LinkedHashMap<>();
    @Mock
    private WrongNotePort repository;
    @Mock
    private FeedbackPolicy feedbackPolicy;
    private WrongNoteUseCase useCase;

    @BeforeEach
    void setUp() {
        lenient().when(feedbackPolicy.getWrongnoteSummaryMaxChars()).thenReturn(255);
        lenient().when(repository.findByPattern(anyString())).thenAnswer(invocation -> Optional.ofNullable(storage.get(invocation.getArgument(0))));
        lenient().when(repository.findAll()).thenAnswer(invocation -> new ArrayList<>(storage.values()));
        lenient().when(repository.save(any(WrongNote.class))).thenAnswer(invocation -> {
            WrongNote note = invocation.getArgument(0);
            storage.put(note.getPattern(), note);
            return note;
        });
        lenient().doAnswer(invocation -> {
            storage.entrySet().removeIf(entry -> entry.getValue().getId().equals(invocation.getArgument(0)));
            return null;
        }).when(repository).deleteById(any());

        useCase = new WrongNoteUseCase(repository, feedbackPolicy);
    }

    @Test
    void addFeedback_incrementsDuplicatePatternCount() {
        useCase.addFeedback(Feedback.of("summary", List.of("Grammar: tense", "Grammar: tense"), "", List.of()));

        WrongNote note = storage.get("Grammar: tense");
        assertEquals(2, note.getCount());
    }

    @Test
    void addFeedback_trimsPatternTo120Chars() {
        String longPoint = "x".repeat(150);

        useCase.addFeedback(Feedback.of("summary", List.of(longPoint), "", List.of()));

        String savedPattern = storage.keySet().iterator().next();
        assertEquals(119, savedPattern.length());
    }

    @Test
    void addFeedback_keepsRecentQueueAt30_andDeletesZeroCountNotes() {
        for (int i = 0; i < 31; i++) {
            useCase.addFeedback(Feedback.of("summary", List.of("pattern-" + i), "", List.of()));
        }

        assertNull(storage.get("pattern-0"));
        assertEquals(30, storage.size());
    }

    @Test
    void addFeedback_decrementsExistingCountWhenEvictedFromQueue() {
        useCase.addFeedback(Feedback.of("summary", List.of("A"), "", List.of()));
        useCase.addFeedback(Feedback.of("summary", List.of("A"), "", List.of()));
        for (int i = 0; i < 29; i++) {
            useCase.addFeedback(Feedback.of("summary", List.of("B-" + i), "", List.of()));
        }

        WrongNote note = storage.get("A");
        assertEquals(1, note.getCount());
    }

    @Test
    void list_returnsDescendingByCount() {
        useCase.addFeedback(Feedback.of("summary", List.of("A"), "", List.of()));
        useCase.addFeedback(Feedback.of("summary", List.of("A"), "", List.of()));
        useCase.addFeedback(Feedback.of("summary", List.of("A"), "", List.of()));
        useCase.addFeedback(Feedback.of("summary", List.of("B"), "", List.of()));

        List<WrongNote> notes = useCase.list();

        assertEquals("A", notes.get(0).getPattern());
        assertEquals(3, notes.get(0).getCount());
        assertEquals(1, notes.get(1).getCount());
    }
}
