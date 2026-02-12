package me.go_gradually.omypic.domain.question;

import java.time.Instant;
import java.util.ArrayList;
import java.util.Collection;
import java.util.Collections;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Set;

public class QuestionGroupAggregate {
    private static final int MAX_QUESTIONS_PER_GROUP = 3;

    private final QuestionGroupId id;
    private String name;
    private final Set<String> tags;
    private final List<QuestionItem> questions;
    private final Instant createdAt;
    private Instant updatedAt;

    private QuestionGroupAggregate(QuestionGroupId id,
                                   String name,
                                   Collection<String> tags,
                                   Collection<QuestionItem> questions,
                                   Instant createdAt,
                                   Instant updatedAt) {
        if (id == null) {
            throw new IllegalArgumentException("QuestionGroup id is required");
        }
        this.id = id;
        this.name = name == null ? "" : name.trim();
        this.tags = normalizeTags(tags);
        this.questions = new ArrayList<>(questions == null ? List.of() : questions);
        if (this.questions.size() > MAX_QUESTIONS_PER_GROUP) {
            throw new IllegalArgumentException("Question group can contain up to 3 questions");
        }
        this.createdAt = createdAt == null ? Instant.now() : createdAt;
        this.updatedAt = updatedAt == null ? this.createdAt : updatedAt;
    }

    public static QuestionGroupAggregate create(String name, Collection<String> tags, Instant now) {
        Instant timestamp = now == null ? Instant.now() : now;
        return new QuestionGroupAggregate(QuestionGroupId.newId(), name, tags, List.of(), timestamp, timestamp);
    }

    public static QuestionGroupAggregate rehydrate(QuestionGroupId id,
                                                   String name,
                                                   Collection<String> tags,
                                                   Collection<QuestionItem> questions,
                                                   Instant createdAt,
                                                   Instant updatedAt) {
        return new QuestionGroupAggregate(id, name, tags, questions, createdAt, updatedAt);
    }

    public void rename(String name, Instant now) {
        this.name = name == null ? "" : name.trim();
        touchUpdatedAt(now);
    }

    public void updateTags(Collection<String> tags, Instant now) {
        this.tags.clear();
        this.tags.addAll(normalizeTags(tags));
        touchUpdatedAt(now);
    }

    public QuestionItem addQuestion(String text, String questionType, Instant now) {
        if (questions.size() >= MAX_QUESTIONS_PER_GROUP) {
            throw new IllegalArgumentException("Question group can contain up to 3 questions");
        }
        QuestionItem item = QuestionItem.create(text, questionType);
        questions.add(item);
        touchUpdatedAt(now);
        return item;
    }

    public void updateQuestion(QuestionItemId itemId, String text, String questionType, Instant now) {
        if (itemId == null) {
            return;
        }
        for (int i = 0; i < questions.size(); i++) {
            QuestionItem existing = questions.get(i);
            if (itemId.equals(existing.getId())) {
                questions.set(i, existing.withTextAndQuestionType(text, questionType));
                touchUpdatedAt(now);
                return;
            }
        }
    }

    public void removeQuestion(QuestionItemId itemId, Instant now) {
        if (itemId == null) {
            return;
        }
        questions.removeIf(question -> itemId.equals(question.getId()));
        touchUpdatedAt(now);
    }

    public boolean hasAnyTag(Set<String> selectedTags) {
        if (selectedTags == null || selectedTags.isEmpty()) {
            return false;
        }
        for (String tag : selectedTags) {
            if (tags.contains(normalizeTag(tag))) {
                return true;
            }
        }
        return false;
    }

    public boolean hasQuestions() {
        return !questions.isEmpty();
    }

    public static Set<String> normalizeTags(Collection<String> rawTags) {
        Set<String> normalized = new LinkedHashSet<>();
        if (rawTags == null) {
            return normalized;
        }
        for (String rawTag : rawTags) {
            String tag = normalizeTag(rawTag);
            if (!tag.isBlank()) {
                normalized.add(tag);
            }
        }
        return normalized;
    }

    public static String normalizeTag(String rawTag) {
        if (rawTag == null) {
            return "";
        }
        return rawTag.trim().toLowerCase();
    }

    private void touchUpdatedAt(Instant now) {
        this.updatedAt = now == null ? Instant.now() : now;
    }

    public QuestionGroupId getId() {
        return id;
    }

    public String getName() {
        return name;
    }

    public Set<String> getTags() {
        return Collections.unmodifiableSet(tags);
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
