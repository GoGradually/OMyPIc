package me.go_gradually.omypic.application.question.port;

import me.go_gradually.omypic.domain.question.QuestionList;
import me.go_gradually.omypic.domain.question.QuestionListId;

import java.util.List;
import java.util.Optional;

public interface QuestionListPort {
    List<QuestionList> findAll();

    Optional<QuestionList> findById(QuestionListId id);

    QuestionList save(QuestionList list);

    void deleteById(QuestionListId id);
}
