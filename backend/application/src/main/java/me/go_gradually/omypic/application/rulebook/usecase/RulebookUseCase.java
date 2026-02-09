package me.go_gradually.omypic.application.rulebook.usecase;

import me.go_gradually.omypic.application.rulebook.model.StoredRulebookFile;
import me.go_gradually.omypic.application.rulebook.policy.RagPolicy;
import me.go_gradually.omypic.application.rulebook.port.RulebookFileStore;
import me.go_gradually.omypic.application.rulebook.port.RulebookIndexPort;
import me.go_gradually.omypic.application.rulebook.port.RulebookPort;
import me.go_gradually.omypic.domain.rulebook.Rulebook;
import me.go_gradually.omypic.domain.rulebook.RulebookContext;
import me.go_gradually.omypic.domain.rulebook.RulebookId;
import me.go_gradually.omypic.domain.shared.util.TextUtils;

import java.io.IOException;
import java.time.Instant;
import java.util.List;
import java.util.Set;
import java.util.stream.Collectors;

public class RulebookUseCase {
    private final RulebookPort repository;
    private final RulebookIndexPort indexPort;
    private final RulebookFileStore fileStore;
    private final RagPolicy ragPolicy;

    public RulebookUseCase(RulebookPort repository,
                           RulebookIndexPort indexPort,
                           RulebookFileStore fileStore,
                           RagPolicy ragPolicy) {
        this.repository = repository;
        this.indexPort = indexPort;
        this.fileStore = fileStore;
        this.ragPolicy = ragPolicy;
    }

    public Rulebook upload(String filename, byte[] bytes) throws IOException {
        if (filename == null || !filename.endsWith(".md")) {
            throw new IllegalArgumentException("Only .md files are supported");
        }
        StoredRulebookFile stored = fileStore.store(filename, bytes);

        Rulebook doc = Rulebook.create(filename, stored.getPath(), Instant.now());
        Rulebook saved = repository.save(doc);

        String text = fileStore.readText(saved.getPath());
        List<String> chunks = TextUtils.splitChunks(text, 800);
        indexPort.indexRulebookChunks(saved.getId(), filename, chunks);
        return saved;
    }

    public List<Rulebook> list() {
        return repository.findAll();
    }

    public Rulebook toggle(String id, boolean enabled) {
        Rulebook doc = repository.findById(RulebookId.of(id)).orElseThrow();
        doc.toggle(enabled, Instant.now());
        return repository.save(doc);
    }

    public void delete(String id) {
        repository.deleteById(RulebookId.of(id));
    }

    public List<RulebookContext> searchContexts(String query) {
        Set<RulebookId> enabledIds = repository.findAll().stream()
                .filter(Rulebook::isEnabled)
                .map(Rulebook::getId)
                .collect(Collectors.toSet());
        if (enabledIds.isEmpty()) {
            return List.of();
        }
        try {
            return indexPort.search(query, ragPolicy.getMaxContextChunks(), enabledIds);
        } catch (IOException e) {
            return List.of();
        }
    }
}
