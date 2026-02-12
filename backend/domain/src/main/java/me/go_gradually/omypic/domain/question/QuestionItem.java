package me.go_gradually.omypic.domain.question;

public final class QuestionItem {
    private final QuestionItemId id;
    private final String text;
    private final QuestionGroup group;

    private QuestionItem(QuestionItemId id, String text, QuestionGroup group) {
        if (id == null) {
            throw new IllegalArgumentException("QuestionItem id is required");
        }
        this.id = id;
        this.text = text == null ? "" : text;
        this.group = group;
    }

    public static QuestionItem rehydrate(QuestionItemId id, String text, QuestionGroup group) {
        return new QuestionItem(id, text, group);
    }

    public static QuestionItem create(String text, QuestionGroup group) {
        return new QuestionItem(QuestionItemId.newId(), text, group);
    }

    public QuestionItem withTextAndGroup(String text, QuestionGroup group) {
        return new QuestionItem(this.id, text, group);
    }

    public QuestionItemId getId() {
        return id;
    }

    public String getText() {
        return text;
    }

    public QuestionGroup getGroup() {
        return group;
    }
}
