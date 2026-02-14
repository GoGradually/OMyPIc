package me.go_gradually.omypic.infrastructure.wrongnote.persistence.mongo;

import me.go_gradually.omypic.application.wrongnote.port.WrongNoteRecentQueuePort;
import org.springframework.stereotype.Component;

import java.time.Instant;
import java.util.ArrayList;
import java.util.List;

@Component
public class WrongNoteRecentQueueMongoAdapter implements WrongNoteRecentQueuePort {
    private static final String GLOBAL_ID = "global";
    private static final int MAX_PATTERNS = 100;
    private final WrongNoteRecentQueueRepository repository;

    public WrongNoteRecentQueueMongoAdapter(WrongNoteRecentQueueRepository repository) {
        this.repository = repository;
    }

    @Override
    public List<String> loadGlobalQueue() {
        return repository.findById(GLOBAL_ID)
                .map(doc -> List.copyOf(doc.getPatterns()))
                .orElse(List.of());
    }

    @Override
    public void saveGlobalQueue(List<String> patterns) {
        // WrongNoteWindow 복원
        WrongNoteRecentQueueDocument doc = repository.findById(GLOBAL_ID).orElseGet(() -> {
            WrongNoteRecentQueueDocument created = new WrongNoteRecentQueueDocument();
            created.setId(GLOBAL_ID);
            return created;
        });

        // 최대 100개까지만 저장
        List<String> input = patterns == null ? List.of() : patterns;
        int fromIndex = Math.max(0, input.size() - MAX_PATTERNS);
        doc.setPatterns(new ArrayList<>(input.subList(fromIndex, input.size())));
        doc.setUpdatedAt(Instant.now());
        repository.save(doc);
    }
}
