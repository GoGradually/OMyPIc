package me.go_gradually.omypic.domain.wrongnote;

import org.junit.jupiter.api.Test;

import java.util.List;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNull;

class WrongNoteWindowTest {

    @Test
    void append_evictsOldestPatternWhenCapacityExceeded() {
        WrongNoteWindow window = WrongNoteWindow.from(List.of("a", "b"), 2);

        String evicted = window.append("c");

        assertEquals("a", evicted);
        assertEquals(List.of("b", "c"), window.snapshot());
    }

    @Test
    void append_returnsNullWhenWithinCapacity() {
        WrongNoteWindow window = WrongNoteWindow.from(List.of("a"), 2);

        String evicted = window.append("b");

        assertNull(evicted);
        assertEquals(List.of("a", "b"), window.snapshot());
    }
}
