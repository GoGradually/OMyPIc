package me.go_gradually.omypic.application.question.port;

import me.go_gradually.omypic.domain.question.QuestionGroupAggregate;
import me.go_gradually.omypic.domain.question.QuestionGroupId;

import java.util.Collection;
import java.util.List;
import java.util.Optional;

public interface QuestionGroupPort {
    List<QuestionGroupAggregate> findAll();

    List<QuestionGroupAggregate> findAllById(Collection<QuestionGroupId> ids);

    Optional<QuestionGroupAggregate> findById(QuestionGroupId id);

    QuestionGroupAggregate save(QuestionGroupAggregate group);

    void deleteById(QuestionGroupId id);

    void deleteAll();
}
