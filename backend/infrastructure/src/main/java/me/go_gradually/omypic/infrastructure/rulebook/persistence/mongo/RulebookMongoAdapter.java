package me.go_gradually.omypic.infrastructure.rulebook.persistence.mongo;

import me.go_gradually.omypic.application.rulebook.port.RulebookPort;
import me.go_gradually.omypic.domain.question.QuestionGroup;
import me.go_gradually.omypic.domain.rulebook.Rulebook;
import me.go_gradually.omypic.domain.rulebook.RulebookId;
import me.go_gradually.omypic.domain.rulebook.RulebookScope;
import org.springframework.stereotype.Component;

import java.util.List;
import java.util.Optional;
import java.util.stream.Collectors;

@Component
public class RulebookMongoAdapter implements RulebookPort {
    private final RulebookMongoRepository repository;

    public RulebookMongoAdapter(RulebookMongoRepository repository) {
        this.repository = repository;
    }

    @Override
    public List<Rulebook> findAll() {
        return repository.findAll().stream()
                .map(this::toDomain)
                .collect(Collectors.toList());
    }

    @Override
    public Optional<Rulebook> findById(RulebookId id) {
        return repository.findById(id.value()).map(this::toDomain);
    }

    @Override
    public Rulebook save(Rulebook rulebook) {
        RulebookDocument saved = repository.save(toDocument(rulebook));
        return toDomain(saved);
    }

    @Override
    public void deleteById(RulebookId id) {
        repository.deleteById(id.value());
    }

    @Override
    public void deleteAll() {
        repository.deleteAll();
    }

    private Rulebook toDomain(RulebookDocument doc) {
        return Rulebook.rehydrate(
                RulebookId.of(doc.getId()),
                doc.getFilename(),
                doc.getPath(),
                doc.getScope(),
                QuestionGroup.fromNullable(doc.getQuestionGroup()),
                doc.isEnabled(),
                doc.getCreatedAt(),
                doc.getUpdatedAt()
        );
    }

    private RulebookDocument toDocument(Rulebook rulebook) {
        RulebookDocument doc = new RulebookDocument();
        doc.setId(rulebook.getId().value());
        doc.setFilename(rulebook.getFilename());
        doc.setPath(rulebook.getPath());
        doc.setScope(rulebook.getScope());
        doc.setQuestionGroup(rulebook.getQuestionGroup() == null ? null : rulebook.getQuestionGroup().value());
        doc.setEnabled(rulebook.isEnabled());
        doc.setCreatedAt(rulebook.getCreatedAt());
        doc.setUpdatedAt(rulebook.getUpdatedAt());
        return doc;
    }
}
