package me.go_gradually.omypic.domain.wrongnote;

import java.util.ArrayDeque;
import java.util.Deque;
import java.util.List;

public final class WrongNoteWindow {
    private final int capacity;
    private final Deque<String> queue;

    private WrongNoteWindow(int capacity, List<String> initialPatterns) {
        if (capacity <= 0) {
            throw new IllegalArgumentException("WrongNoteWindow capacity must be positive");
        }
        this.capacity = capacity;
        this.queue = new ArrayDeque<>();
        /*
        현재
         */
        if (initialPatterns != null) {
            for (String pattern : initialPatterns) {
                if (pattern == null || pattern.isBlank()) {
                    continue;
                }
                this.queue.addLast(pattern);
            }
        }
        trimToCapacity();
    }

    public static WrongNoteWindow from(List<String> initialPatterns, int capacity) {
        return new WrongNoteWindow(capacity, initialPatterns);
    }

    public String append(String pattern) {
        if (pattern == null || pattern.isBlank()) {
            return null;
        }
        queue.addLast(pattern);
        if (queue.size() <= capacity) {
            return null;
        }
        return queue.removeFirst();
    }

    public List<String> snapshot() {
        return List.copyOf(queue);
    }

    private void trimToCapacity() {
        while (queue.size() > capacity) {
            queue.removeFirst();
        }
    }
}
