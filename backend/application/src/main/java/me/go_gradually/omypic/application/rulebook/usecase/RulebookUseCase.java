package me.go_gradually.omypic.application.rulebook.usecase;

import me.go_gradually.omypic.application.rulebook.model.StoredRulebookFile;
import me.go_gradually.omypic.application.rulebook.policy.RagPolicy;
import me.go_gradually.omypic.application.rulebook.port.RulebookFileStore;
import me.go_gradually.omypic.application.rulebook.port.RulebookIndexPort;
import me.go_gradually.omypic.application.rulebook.port.RulebookPort;
import me.go_gradually.omypic.application.shared.port.MetricsPort;
import me.go_gradually.omypic.domain.question.QuestionGroup;
import me.go_gradually.omypic.domain.rulebook.Rulebook;
import me.go_gradually.omypic.domain.rulebook.RulebookContext;
import me.go_gradually.omypic.domain.rulebook.RulebookId;
import me.go_gradually.omypic.domain.rulebook.RulebookScope;
import me.go_gradually.omypic.domain.shared.util.TextUtils;

import java.io.IOException;
import java.time.Duration;
import java.time.Instant;
import java.util.ArrayList;
import java.util.List;
import java.util.Set;
import java.util.stream.Collectors;

public class RulebookUseCase {
    private final RulebookPort repository;
    private final RulebookIndexPort indexPort;
    private final RulebookFileStore fileStore;
    private final RagPolicy ragPolicy;
    private final MetricsPort metrics;

    public RulebookUseCase(RulebookPort repository,
                           RulebookIndexPort indexPort,
                           RulebookFileStore fileStore,
                           RagPolicy ragPolicy,
                           MetricsPort metrics) {
        this.repository = repository;
        this.indexPort = indexPort;
        this.fileStore = fileStore;
        this.ragPolicy = ragPolicy;
        this.metrics = metrics;
    }

    public Rulebook upload(String filename, byte[] bytes, RulebookScope scope, String questionGroup) throws IOException {
        Instant start = Instant.now();
        validateMarkdownFilename(filename);
        Rulebook saved = storeRulebook(filename, bytes, scope, questionGroup);
        indexRulebook(saved, filename);
        metrics.recordRulebookUploadLatency(Duration.between(start, Instant.now()));
        return saved;
    }

    private void validateMarkdownFilename(String filename) {
        if (filename == null || !filename.endsWith(".md")) {
            throw new IllegalArgumentException("Only .md files are supported");
        }
    }

    private Rulebook storeRulebook(String filename, byte[] bytes, RulebookScope scope, String questionGroup) throws IOException {
        RulebookScope resolvedScope = scope == null ? RulebookScope.MAIN : scope;
        QuestionGroup resolvedQuestionGroup = QuestionGroup.fromNullable(questionGroup);
        Rulebook.create(filename, "", resolvedScope, resolvedQuestionGroup, Instant.now());
        StoredRulebookFile stored = fileStore.store(filename, bytes);
        Rulebook doc = Rulebook.create(filename, stored.path(), resolvedScope, resolvedQuestionGroup, Instant.now());
        return repository.save(doc);
    }

    private void indexRulebook(Rulebook saved, String filename) throws IOException {
        String text = fileStore.readText(saved.getPath());
        List<String> chunks = TextUtils.splitChunks(text, 800);
        indexPort.indexRulebookChunks(saved.getId(), filename, chunks);
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
        return searchByIds(query, ragPolicy.getMaxContextChunks(), enabledIds);
    }

    public List<RulebookContext> searchContextsForTurn(QuestionGroup questionGroup, String query, int maxDocuments) {
        if (maxDocuments <= 0) {
            return List.of();
        }
        List<Rulebook> enabledRulebooks = enabledRulebooks();
        if (enabledRulebooks.isEmpty()) {
            return List.of();
        }
        Set<RulebookId> mainIds = mainRulebookIds(enabledRulebooks);
        Set<RulebookId> questionIds = questionRulebookIds(enabledRulebooks, questionGroup);
        return selectContextsForTurn(query, maxDocuments, mainIds, questionIds);
    }

    private List<Rulebook> enabledRulebooks() {
        return repository.findAll().stream().filter(Rulebook::isEnabled).toList();
    }

    private Set<RulebookId> mainRulebookIds(List<Rulebook> enabledRulebooks) {
        return enabledRulebooks.stream()
                .filter(rulebook -> rulebook.getScope() == RulebookScope.MAIN)
                .map(Rulebook::getId)
                .collect(Collectors.toSet());
    }

    private Set<RulebookId> questionRulebookIds(List<Rulebook> enabledRulebooks, QuestionGroup questionGroup) {
        return enabledRulebooks.stream()
                .filter(rulebook -> rulebook.getScope() == RulebookScope.QUESTION)
                .filter(rulebook -> questionGroup != null && questionGroup.equals(rulebook.getQuestionGroup()))
                .map(Rulebook::getId)
                .collect(Collectors.toSet());
    }

    private List<RulebookContext> selectContextsForTurn(String query,
                                                        int maxDocuments,
                                                        Set<RulebookId> mainIds,
                                                        Set<RulebookId> questionIds) {
        List<RulebookContext> selected = new ArrayList<>();
        addUnique(selected, searchByIds(query, 1, mainIds), maxDocuments);
        addUnique(selected, searchByIds(query, maxDocuments - selected.size(), questionIds), maxDocuments);
        addUnique(selected, searchByIds(query, maxDocuments, mainIds), maxDocuments);
        addUnique(selected, searchByIds(query, maxDocuments, questionIds), maxDocuments);
        return selected;
    }

    private List<RulebookContext> searchByIds(String query, int limit, Set<RulebookId> ids) {
        if (ids == null || ids.isEmpty() || limit <= 0) {
            return List.of();
        }
        try {
            return indexPort.search(query, limit, ids);
        } catch (IOException e) {
            return List.of();
        }
    }

    private void addUnique(List<RulebookContext> target, List<RulebookContext> candidates, int limit) {
        for (RulebookContext candidate : candidates) {
            if (!hasCapacity(target, limit)) {
                return;
            }
            if (isDuplicate(target, candidate)) {
                continue;
            }
            target.add(candidate);
        }
    }

    private boolean hasCapacity(List<RulebookContext> target, int limit) {
        return target.size() < limit;
    }

    private boolean isDuplicate(List<RulebookContext> target, RulebookContext candidate) {
        return target.stream()
                .anyMatch(existing -> existing.rulebookId().equals(candidate.rulebookId())
                        && existing.text().equals(candidate.text()));
    }

}
