package me.go_gradually.omypic.infrastructure.wrongnote.persistence.mongo;

import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.extension.ExtendWith;
import org.mockito.Mock;
import org.mockito.junit.jupiter.MockitoExtension;

import java.util.List;
import java.util.Optional;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.mockito.ArgumentMatchers.any;
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
    void saveGlobalQueue_clampsTo30() {
        when(repository.findById("global")).thenReturn(Optional.empty());

        adapter.saveGlobalQueue(java.util.stream.IntStream.range(0, 40).mapToObj(i -> "p-" + i).toList());

        verify(repository).save(any(WrongNoteRecentQueueDocument.class));
    }
}
