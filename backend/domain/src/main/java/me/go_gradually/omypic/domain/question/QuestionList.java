package me.go_gradually.omypic.domain.question;

import java.time.Instant;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.Map;
import java.util.stream.Collectors;

public class QuestionList {
    private final QuestionListId id;
    private String name;
    private final List<QuestionItem> questions;
    private final Instant createdAt;
    private Instant updatedAt;

    private QuestionList(QuestionListId id, String name, List<QuestionItem> questions, Instant createdAt, Instant updatedAt) {
        if (id == null) {
            throw new IllegalArgumentException("QuestionList id is required");
        }
        this.id = id;
        this.name = name == null ? "" : name;
        this.questions = new ArrayList<>(questions == null ? List.of() : questions);
        this.createdAt = createdAt == null ? Instant.now() : createdAt;
        this.updatedAt = updatedAt == null ? this.createdAt : updatedAt;
    }

    public static QuestionList create(String name, Instant now) {
        Instant timestamp = now == null ? Instant.now() : now;
        return new QuestionList(QuestionListId.newId(), name, List.of(), timestamp, timestamp);
    }

    public static QuestionList rehydrate(QuestionListId id, String name, List<QuestionItem> questions, Instant createdAt, Instant updatedAt) {
        return new QuestionList(id, name, questions, createdAt, updatedAt);
    }

    public void rename(String name, Instant now) {
        this.name = name == null ? "" : name;
        touchUpdatedAt(now);
    }

    public QuestionItem addQuestion(String text, QuestionGroup group, Instant now) {
        if (questions.size() >= 200) {
            throw new IllegalArgumentException("Question limit exceeded");
        }
        QuestionItem item = QuestionItem.create(text, group);
        questions.add(item);
        touchUpdatedAt(now);
        return item;
    }

    public void updateQuestion(QuestionItemId itemId, String text, QuestionGroup group, Instant now) {
        if (itemId == null) {
            return;
        }
        for (int i = 0; i < questions.size(); i++) {
            QuestionItem existing = questions.get(i);
            if (itemId.equals(existing.getId())) {
                questions.set(i, existing.withTextAndGroup(text, group));
            }
        }
        touchUpdatedAt(now);
    }

    public void removeQuestion(QuestionItemId itemId, Instant now) {
        if (itemId != null && questions.size() <= 1 && questions.stream().anyMatch(q -> itemId.equals(q.getId()))) {
            throw new IllegalStateException("Question list must contain at least 1 question");
        }
        if (itemId != null) {
            questions.removeIf(q -> itemId.equals(q.getId()));
        }
        touchUpdatedAt(now);
    }

    public Map<QuestionGroup, List<QuestionItemId>> groupQuestionIdsByGroup() {
        return questions.stream()
                .filter(q -> q.getGroup() != null)
                .collect(Collectors.groupingBy(QuestionItem::getGroup,
                        Collectors.mapping(QuestionItem::getId, Collectors.toList())));
    }

    private void touchUpdatedAt(Instant now) {
        this.updatedAt = now == null ? Instant.now() : now;
    }

    public QuestionListId getId() {
        return id;
    }

    public String getName() {
        return name;
    }

    public List<QuestionItem> getQuestions() {
        return Collections.unmodifiableList(questions);
    }

    public Instant getCreatedAt() {
        return createdAt;
    }

    public Instant getUpdatedAt() {
        return updatedAt;
    }
}
