package me.go_gradually.omypic.application.question.usecase;

import me.go_gradually.omypic.application.question.model.NextQuestion;
import me.go_gradually.omypic.application.question.port.QuestionListPort;
import me.go_gradually.omypic.application.session.port.SessionStorePort;
import me.go_gradually.omypic.application.shared.port.MetricsPort;
import me.go_gradually.omypic.domain.question.QuestionItem;
import me.go_gradually.omypic.domain.question.QuestionGroup;
import me.go_gradually.omypic.domain.question.QuestionItemId;
import me.go_gradually.omypic.domain.question.QuestionList;
import me.go_gradually.omypic.domain.question.QuestionListId;
import me.go_gradually.omypic.domain.session.SessionId;
import me.go_gradually.omypic.domain.session.SessionState;

import java.time.Duration;
import java.time.Instant;
import java.util.List;
import java.util.Optional;

public class QuestionUseCase {
    private static final String MOCK_EXAM_EXHAUSTED = "QUESTION_EXHAUSTED";

    private final QuestionListPort repository;
    private final SessionStorePort sessionStore;
    private final MetricsPort metrics;

    public QuestionUseCase(QuestionListPort repository, SessionStorePort sessionStore, MetricsPort metrics) {
        this.repository = repository;
        this.sessionStore = sessionStore;
        this.metrics = metrics;
    }

    public List<QuestionList> list() {
        return repository.findAll();
    }

    public QuestionList create(String name) {
        QuestionList doc = QuestionList.create(name, Instant.now());
        return repository.save(doc);
    }

    public QuestionList updateName(String id, String name) {
        QuestionList doc = repository.findById(QuestionListId.of(id)).orElseThrow();
        doc.rename(name, Instant.now());
        return repository.save(doc);
    }

    public void delete(String id) {
        repository.deleteById(QuestionListId.of(id));
    }

    public QuestionList addQuestion(String listId, String text, String group) {
        QuestionList doc = repository.findById(QuestionListId.of(listId)).orElseThrow();
        doc.addQuestion(text, QuestionGroup.fromNullable(group), Instant.now());
        return repository.save(doc);
    }

    public QuestionList updateQuestion(String listId, String itemId, String text, String group) {
        QuestionList doc = repository.findById(QuestionListId.of(listId)).orElseThrow();
        doc.updateQuestion(QuestionItemId.of(itemId), text, QuestionGroup.fromNullable(group), Instant.now());
        return repository.save(doc);
    }

    public QuestionList deleteQuestion(String listId, String itemId) {
        QuestionList doc = repository.findById(QuestionListId.of(listId)).orElseThrow();
        doc.removeQuestion(QuestionItemId.of(itemId), Instant.now());
        return repository.save(doc);
    }

    public NextQuestion nextQuestion(String listId, String sessionId) {
        Instant start = Instant.now();
        QuestionList doc = repository.findById(QuestionListId.of(listId)).orElseThrow();
        if (doc.getQuestions().isEmpty()) {
            throw new IllegalStateException("Question list must contain at least 1 question");
        }
        SessionState session = sessionStore.getOrCreate(SessionId.of(sessionId));
        Optional<QuestionItem> selected = session.nextQuestion(doc);
        NextQuestion response = selected.map(item -> {
            NextQuestion next = new NextQuestion();
            next.setQuestionId(item.getId().value());
            next.setText(item.getText());
            next.setGroup(item.getGroup() == null ? null : item.getGroup().value());
            return next;
        }).orElseGet(NextQuestion::skipped);
        response.setMockExamCompleted(session.isMockExamCompleted());
        if (session.isMockExamCompleted()) {
            response.setMockExamCompletionReason(MOCK_EXAM_EXHAUSTED);
        }
        metrics.recordQuestionNextLatency(Duration.between(start, Instant.now()));
        return response;
    }
}
