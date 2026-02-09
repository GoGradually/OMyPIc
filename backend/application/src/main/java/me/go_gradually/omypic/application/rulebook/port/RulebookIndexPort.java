package me.go_gradually.omypic.application.rulebook.port;

import me.go_gradually.omypic.domain.rulebook.RulebookContext;
import me.go_gradually.omypic.domain.rulebook.RulebookId;

import java.io.IOException;
import java.util.List;
import java.util.Set;

public interface RulebookIndexPort {
    void indexRulebookChunks(RulebookId rulebookId, String filename, List<String> chunks) throws IOException;

    List<RulebookContext> search(String query, int topK, Set<RulebookId> enabledRulebookIds) throws IOException;
}
