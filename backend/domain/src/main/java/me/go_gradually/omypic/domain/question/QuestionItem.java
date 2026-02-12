package me.go_gradually.omypic.domain.question;

public final class QuestionItem {
    private final QuestionItemId id;
    private final String text;
    private final String questionType;

    private QuestionItem(QuestionItemId id, String text, String questionType) {
        if (id == null) {
            throw new IllegalArgumentException("QuestionItem id is required");
        }
        this.id = id;
        this.text = text == null ? "" : text;
        this.questionType = normalizeQuestionType(questionType);
    }

    public static QuestionItem rehydrate(QuestionItemId id, String text, String questionType) {
        return new QuestionItem(id, text, questionType);
    }

    public static QuestionItem create(String text, String questionType) {
        return new QuestionItem(QuestionItemId.newId(), text, questionType);
    }

    public QuestionItem withTextAndQuestionType(String text, String questionType) {
        return new QuestionItem(this.id, text, questionType);
    }

    private static String normalizeQuestionType(String questionType) {
        if (questionType == null) {
            return null;
        }
        String normalized = questionType.trim();
        return normalized.isBlank() ? null : normalized;
    }

    public QuestionItemId getId() {
        return id;
    }

    public String getText() {
        return text;
    }

    public String getQuestionType() {
        return questionType;
    }
}
