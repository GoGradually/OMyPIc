package me.go_gradually.omypic.infrastructure.question.persistence.mongo;

import me.go_gradually.omypic.application.question.port.QuestionGroupPort;
import me.go_gradually.omypic.domain.question.QuestionGroupAggregate;
import me.go_gradually.omypic.domain.question.QuestionGroupId;
import me.go_gradually.omypic.domain.question.QuestionItem;
import me.go_gradually.omypic.domain.question.QuestionItemId;
import org.springframework.stereotype.Component;

import java.util.Collection;
import java.util.List;
import java.util.Optional;
import java.util.stream.Collectors;

@Component
public class QuestionGroupMongoAdapter implements QuestionGroupPort {
    private final QuestionGroupMongoRepository repository;

    public QuestionGroupMongoAdapter(QuestionGroupMongoRepository repository) {
        this.repository = repository;
    }

    @Override
    public List<QuestionGroupAggregate> findAll() {
        return repository.findAll().stream()
                .map(this::toDomain)
                .collect(Collectors.toList());
    }

    @Override
    public List<QuestionGroupAggregate> findAllById(Collection<QuestionGroupId> ids) {
        if (ids == null || ids.isEmpty()) {
            return List.of();
        }
        List<String> values = ids.stream().map(QuestionGroupId::value).toList();
        return repository.findAllById(values).stream()
                .map(this::toDomain)
                .toList();
    }

    @Override
    public Optional<QuestionGroupAggregate> findById(QuestionGroupId id) {
        return repository.findById(id.value()).map(this::toDomain);
    }

    @Override
    public QuestionGroupAggregate save(QuestionGroupAggregate group) {
        QuestionGroupDocument saved = repository.save(toDocument(group));
        return toDomain(saved);
    }

    @Override
    public void deleteById(QuestionGroupId id) {
        repository.deleteById(id.value());
    }

    @Override
    public void deleteAll() {
        repository.deleteAll();
    }

    private QuestionGroupAggregate toDomain(QuestionGroupDocument doc) {
        List<QuestionItemDocument> items = doc.getQuestions() == null ? List.of() : doc.getQuestions();
        List<QuestionItem> questions = items.stream().map(this::toDomainItem).collect(Collectors.toList());
        return rehydrate(doc, questions);
    }

    private QuestionGroupAggregate rehydrate(QuestionGroupDocument doc, List<QuestionItem> questions) {
        return QuestionGroupAggregate.rehydrate(
                QuestionGroupId.of(doc.getId()), doc.getName(), doc.getTags(), questions, doc.getCreatedAt(), doc.getUpdatedAt()
        );
    }

    private QuestionGroupDocument toDocument(QuestionGroupAggregate group) {
        QuestionGroupDocument doc = new QuestionGroupDocument();
        doc.setId(group.getId().value());
        doc.setName(group.getName());
        doc.setTags(group.getTags().stream().sorted().toList());
        doc.setQuestions(group.getQuestions().stream()
                .map(this::toDocumentItem)
                .collect(Collectors.toList()));
        doc.setCreatedAt(group.getCreatedAt());
        doc.setUpdatedAt(group.getUpdatedAt());
        return doc;
    }

    private QuestionItem toDomainItem(QuestionItemDocument doc) {
        return QuestionItem.rehydrate(
                QuestionItemId.of(doc.getId()),
                doc.getText(),
                doc.getQuestionType()
        );
    }

    private QuestionItemDocument toDocumentItem(QuestionItem item) {
        QuestionItemDocument doc = new QuestionItemDocument();
        doc.setId(item.getId().value());
        doc.setText(item.getText());
        doc.setQuestionType(item.getQuestionType());
        return doc;
    }
}
