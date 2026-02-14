package me.go_gradually.omypic.infrastructure.wrongnote.persistence.mongo;

import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.extension.ExtendWith;
import org.mockito.ArgumentCaptor;
import org.mockito.Mock;
import org.mockito.junit.jupiter.MockitoExtension;

import java.util.List;
import java.util.Optional;
import java.util.stream.IntStream;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.mockito.Mockito.verify;
import static org.mockito.Mockito.when;

@ExtendWith(MockitoExtension.class)
class WrongNoteRecentQueueMongoAdapterTest {

    @Mock
    private WrongNoteRecentQueueRepository repository;

    private WrongNoteRecentQueueMongoAdapter adapter;

    @BeforeEach
    void setUp() {
        adapter = new WrongNoteRecentQueueMongoAdapter(repository);
    }

    @Test
    void loadGlobalQueue_returnsEmptyWhenMissing() {
        when(repository.findById("global")).thenReturn(Optional.empty());

        List<String> result = adapter.loadGlobalQueue();

        assertEquals(List.of(), result);
    }

    @Test
    void saveGlobalQueue_clampsTo100() {
        when(repository.findById("global")).thenReturn(Optional.empty());

        adapter.saveGlobalQueue(IntStream.range(0, 140).mapToObj(i -> "p-" + i).toList());

        ArgumentCaptor<WrongNoteRecentQueueDocument> captor = ArgumentCaptor.forClass(WrongNoteRecentQueueDocument.class);
        verify(repository).save(captor.capture());

        WrongNoteRecentQueueDocument saved = captor.getValue();
        assertEquals("global", saved.getId());
        assertEquals(100, saved.getPatterns().size());
        assertEquals("p-40", saved.getPatterns().get(0));
        assertEquals("p-139", saved.getPatterns().get(99));
    }
}
