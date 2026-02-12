package me.go_gradually.omypic.application.question.usecase;

import me.go_gradually.omypic.application.question.model.NextQuestion;
import me.go_gradually.omypic.application.question.model.QuestionTagStat;
import me.go_gradually.omypic.application.question.port.QuestionGroupPort;
import me.go_gradually.omypic.application.session.port.SessionStorePort;
import me.go_gradually.omypic.application.shared.port.MetricsPort;
import me.go_gradually.omypic.domain.question.QuestionGroupAggregate;
import me.go_gradually.omypic.domain.question.QuestionGroupId;
import me.go_gradually.omypic.domain.question.QuestionItem;
import me.go_gradually.omypic.domain.question.QuestionItemId;
import me.go_gradually.omypic.domain.session.SessionId;
import me.go_gradually.omypic.domain.session.SessionState;

import java.time.Duration;
import java.time.Instant;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

public class QuestionUseCase {
    private final QuestionGroupPort repository;
    private final SessionStorePort sessionStore;
    private final MetricsPort metrics;

    public QuestionUseCase(QuestionGroupPort repository, SessionStorePort sessionStore, MetricsPort metrics) {
        this.repository = repository;
        this.sessionStore = sessionStore;
        this.metrics = metrics;
    }

    public List<QuestionGroupAggregate> list() {
        return repository.findAll();
    }

    public QuestionGroupAggregate createGroup(String name, List<String> tags) {
        QuestionGroupAggregate group = QuestionGroupAggregate.create(name, tags, Instant.now());
        return repository.save(group);
    }

    public QuestionGroupAggregate updateGroup(String id, String name, List<String> tags) {
        QuestionGroupAggregate group = repository.findById(QuestionGroupId.of(id)).orElseThrow();
        group.rename(name, Instant.now());
        group.updateTags(tags, Instant.now());
        return repository.save(group);
    }

    public void deleteGroup(String id) {
        repository.deleteById(QuestionGroupId.of(id));
    }

    public QuestionGroupAggregate addQuestion(String groupId, String text, String questionType) {
        QuestionGroupAggregate group = repository.findById(QuestionGroupId.of(groupId)).orElseThrow();
        group.addQuestion(text, questionType, Instant.now());
        return repository.save(group);
    }

    public QuestionGroupAggregate updateQuestion(String groupId, String itemId, String text, String questionType) {
        QuestionGroupAggregate group = repository.findById(QuestionGroupId.of(groupId)).orElseThrow();
        group.updateQuestion(QuestionItemId.of(itemId), text, questionType, Instant.now());
        return repository.save(group);
    }

    public QuestionGroupAggregate deleteQuestion(String groupId, String itemId) {
        QuestionGroupAggregate group = repository.findById(QuestionGroupId.of(groupId)).orElseThrow();
        group.removeQuestion(QuestionItemId.of(itemId), Instant.now());
        return repository.save(group);
    }

    public List<QuestionTagStat> listTagStats() {
        Map<String, Long> counts = new LinkedHashMap<>();
        for (QuestionGroupAggregate group : repository.findAll()) {
            for (String tag : group.getTags()) {
                counts.merge(tag, 1L, Long::sum);
            }
        }
        return counts.entrySet().stream()
                .sorted(Map.Entry.comparingByKey())
                .map(entry -> new QuestionTagStat(entry.getKey(), entry.getValue(), entry.getValue() > 0L))
                .toList();
    }

    public NextQuestion nextQuestion(String sessionId) {
        Instant start = Instant.now();
        SessionState session = sessionStore.getOrCreate(SessionId.of(sessionId));
        if (session.getSelectedGroupTags().isEmpty()) {
            throw new IllegalStateException("질문 그룹 태그를 선택하고 학습 모드를 적용해 주세요.");
        }

        List<QuestionGroupAggregate> allGroups = repository.findAll();
        Map<String, QuestionGroupAggregate> groupsById = allGroups.stream()
                .collect(java.util.stream.Collectors.toMap(group -> group.getId().value(), group -> group, (left, right) -> left));

        NextQuestion response = NextQuestion.skipped();
        while (true) {
            String groupId = session.currentCandidateGroupId();
            if (groupId == null) {
                break;
            }
            QuestionGroupAggregate group = groupsById.get(groupId);
            if (group == null || group.getQuestions().isEmpty()) {
                session.moveToNextGroup();
                continue;
            }

            int questionIndex = session.getCurrentQuestionIndex(groupId);
            if (questionIndex >= group.getQuestions().size()) {
                session.moveToNextGroup();
                continue;
            }

            QuestionItem item = group.getQuestions().get(questionIndex);
            session.markQuestionAsked(groupId);
            response.setSkipped(false);
            response.setQuestionId(item.getId().value());
            response.setText(item.getText());
            response.setGroupId(group.getId().value());
            response.setGroup(group.getName());
            response.setQuestionType(item.getQuestionType());
            break;
        }

        metrics.recordQuestionNextLatency(Duration.between(start, Instant.now()));
        return response;
    }
}
