package me.go_gradually.omypic.application.wrongnote.usecase;

import me.go_gradually.omypic.application.feedback.policy.FeedbackPolicy;
import me.go_gradually.omypic.application.wrongnote.port.WrongNotePort;
import me.go_gradually.omypic.application.wrongnote.port.WrongNoteRecentQueuePort;
import me.go_gradually.omypic.domain.feedback.Feedback;
import me.go_gradually.omypic.domain.wrongnote.WrongNote;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.extension.ExtendWith;
import org.mockito.Mock;
import org.mockito.junit.jupiter.MockitoExtension;

import java.util.*;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.mockito.ArgumentMatchers.any;
import static org.mockito.ArgumentMatchers.anyString;
import static org.mockito.Mockito.doAnswer;
import static org.mockito.Mockito.when;
import static org.mockito.Mockito.verify;

@ExtendWith(MockitoExtension.class)
class WrongNoteUseCaseTest {

    private final Map<String, WrongNote> storage = new LinkedHashMap<>();
    private final List<String> queueStorage = new ArrayList<>();
    @Mock
    private WrongNotePort repository;
    @Mock
    private WrongNoteRecentQueuePort recentQueueStore;
    @Mock
    private FeedbackPolicy feedbackPolicy;
    private WrongNoteUseCase useCase;

    @BeforeEach
    void setUp() {
        useCase = new WrongNoteUseCase(repository, recentQueueStore, feedbackPolicy);
    }

    @Test
    void addFeedback_incrementsDuplicatePatternCount() {
        stubAddFeedbackDependencies();
        useCase.addFeedback(Feedback.of("summary", List.of("Grammar: tense", "Grammar: tense"), "", List.of()));

        WrongNote note = storage.get("Grammar: tense");
        assertEquals(2, note.getCount());
    }

    @Test
    void addFeedback_trimsPatternTo120Chars() {
        stubAddFeedbackDependencies();
        String longPoint = "x".repeat(150);

        useCase.addFeedback(Feedback.of("summary", List.of(longPoint), "", List.of()));

        String savedPattern = storage.keySet().iterator().next();
        assertEquals(119, savedPattern.length());
    }

    @Test
    void addFeedback_keepsRecentQueueAt30_andDeletesZeroCountNotes() {
        stubAddFeedbackDependencies();
        stubDeleteByIdFromStorage();
        for (int i = 0; i < 31; i++) {
            useCase.addFeedback(Feedback.of("summary", List.of("pattern-" + i), "", List.of()));
        }

        assertNull(storage.get("pattern-0"));
        assertEquals(30, storage.size());
    }

    @Test
    void addFeedback_appliesConfiguredWindowSize() {
        stubAddFeedbackDependencies();
        when(feedbackPolicy.getWrongnoteWindowSize()).thenReturn(2);
        stubDeleteByIdFromStorage();

        useCase.addFeedback(Feedback.of("summary", List.of("A"), "", List.of()));
        useCase.addFeedback(Feedback.of("summary", List.of("B"), "", List.of()));
        useCase.addFeedback(Feedback.of("summary", List.of("C"), "", List.of()));

        assertEquals(List.of("B", "C"), queueStorage);
        assertNull(storage.get("A"));
    }

    @Test
    void addFeedback_decrementsExistingCountWhenEvictedFromQueue() {
        stubAddFeedbackDependencies();
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
        stubAddFeedbackDependencies();
        stubFindAllFromStorage();
        useCase.addFeedback(Feedback.of("summary", List.of("A"), "", List.of()));
        useCase.addFeedback(Feedback.of("summary", List.of("A"), "", List.of()));
        useCase.addFeedback(Feedback.of("summary", List.of("A"), "", List.of()));
        useCase.addFeedback(Feedback.of("summary", List.of("B"), "", List.of()));

        List<WrongNote> notes = useCase.list();

        assertEquals("A", notes.get(0).getPattern());
        assertEquals(3, notes.get(0).getCount());
        assertEquals(1, notes.get(1).getCount());
    }

    @Test
    void addFeedback_savesQueueState() {
        stubAddFeedbackDependencies();
        useCase.addFeedback(Feedback.of("summary", List.of("A"), "", List.of()));

        verify(recentQueueStore).saveGlobalQueue(any());
    }

    @Test
    void addFeedback_recordsRecommendationPoints() {
        stubAddFeedbackDependencies();
        useCase.addFeedback(Feedback.of(
                "summary",
                List.of(
                        "Grammar: tense",
                        "Expression: clearer wording",
                        "Logic: add reason"
                ),
                List.of(
                        "Filler: Well: Use this to start naturally.",
                        "Adjective: impressive - Use it for vivid detail.",
                        "Adverb: definitely - Use it for confidence."
                ),
                "",
                List.of()
        ));

        WrongNote fillerNote = storage.get("Filler: Well: Use this to start naturally.");
        WrongNote adjectiveNote = storage.get("Adjective: impressive - Use it for vivid detail.");
        WrongNote adverbNote = storage.get("Adverb: definitely - Use it for confidence.");
        assertNotNull(fillerNote);
        assertNotNull(adjectiveNote);
        assertNotNull(adverbNote);
        assertEquals(1, fillerNote.getCount());
        assertEquals(1, adjectiveNote.getCount());
        assertEquals(1, adverbNote.getCount());
    }

    private void stubAddFeedbackDependencies() {
        when(feedbackPolicy.getWrongnoteSummaryMaxChars()).thenReturn(255);
        when(feedbackPolicy.getWrongnoteWindowSize()).thenReturn(30);
        when(repository.findByPattern(anyString())).thenAnswer(invocation -> Optional.ofNullable(storage.get(invocation.getArgument(0))));
        when(repository.save(any(WrongNote.class))).thenAnswer(invocation -> {
            WrongNote note = invocation.getArgument(0);
            storage.put(note.getPattern(), note);
            return note;
        });
        when(recentQueueStore.loadGlobalQueue()).thenAnswer(invocation -> List.copyOf(queueStorage));
        doAnswer(invocation -> {
            List<String> saved = invocation.getArgument(0);
            queueStorage.clear();
            queueStorage.addAll(saved);
            return null;
        }).when(recentQueueStore).saveGlobalQueue(any());
    }

    private void stubDeleteByIdFromStorage() {
        doAnswer(invocation -> {
            storage.entrySet().removeIf(entry -> entry.getValue().getId().equals(invocation.getArgument(0)));
            return null;
        }).when(repository).deleteById(any());
    }

    private void stubFindAllFromStorage() {
        when(repository.findAll()).thenAnswer(invocation -> new ArrayList<>(storage.values()));
    }
}
