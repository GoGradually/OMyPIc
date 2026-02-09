package me.go_gradually.omypic.infrastructure.question.persistence.mongo;

import me.go_gradually.omypic.application.question.port.QuestionListPort;
import me.go_gradually.omypic.domain.question.QuestionItem;
import me.go_gradually.omypic.domain.question.QuestionItemId;
import me.go_gradually.omypic.domain.question.QuestionList;
import me.go_gradually.omypic.domain.question.QuestionListId;
import org.springframework.stereotype.Component;

import java.util.List;
import java.util.Optional;
import java.util.stream.Collectors;

@Component
public class QuestionListMongoAdapter implements QuestionListPort {
    private final QuestionListMongoRepository repository;

    public QuestionListMongoAdapter(QuestionListMongoRepository repository) {
        this.repository = repository;
    }

    @Override
    public List<QuestionList> findAll() {
        return repository.findAll().stream()
                .map(this::toDomain)
                .collect(Collectors.toList());
    }

    @Override
    public Optional<QuestionList> findById(QuestionListId id) {
        return repository.findById(id.value()).map(this::toDomain);
    }

    @Override
    public QuestionList save(QuestionList list) {
        QuestionListDocument saved = repository.save(toDocument(list));
        return toDomain(saved);
    }

    @Override
    public void deleteById(QuestionListId id) {
        repository.deleteById(id.value());
    }

    private QuestionList toDomain(QuestionListDocument doc) {
        java.util.List<QuestionItemDocument> items = doc.getQuestions() == null ? java.util.List.of() : doc.getQuestions();
        return QuestionList.rehydrate(
                QuestionListId.of(doc.getId()),
                doc.getName(),
                items.stream()
                        .map(this::toDomainItem)
                        .collect(java.util.stream.Collectors.toList()),
                doc.getCreatedAt(),
                doc.getUpdatedAt()
        );
    }

    private QuestionListDocument toDocument(QuestionList list) {
        QuestionListDocument doc = new QuestionListDocument();
        doc.setId(list.getId().value());
        doc.setName(list.getName());
        doc.setQuestions(list.getQuestions().stream()
                .map(this::toDocumentItem)
                .collect(java.util.stream.Collectors.toList()));
        doc.setCreatedAt(list.getCreatedAt());
        doc.setUpdatedAt(list.getUpdatedAt());
        return doc;
    }

    private QuestionItem toDomainItem(QuestionItemDocument doc) {
        return QuestionItem.rehydrate(
                QuestionItemId.of(doc.getId()),
                doc.getText(),
                doc.getGroup()
        );
    }

    private QuestionItemDocument toDocumentItem(QuestionItem item) {
        QuestionItemDocument doc = new QuestionItemDocument();
        doc.setId(item.getId().value());
        doc.setText(item.getText());
        doc.setGroup(item.getGroup());
        return doc;
    }
}
