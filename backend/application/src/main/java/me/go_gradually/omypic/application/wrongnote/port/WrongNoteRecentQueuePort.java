package me.go_gradually.omypic.application.wrongnote.port;

import java.util.List;

public interface WrongNoteRecentQueuePort {
    List<String> loadGlobalQueue();

    void saveGlobalQueue(List<String> patterns);

    void clearGlobalQueue();
}
