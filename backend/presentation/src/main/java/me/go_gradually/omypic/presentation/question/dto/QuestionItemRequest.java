package me.go_gradually.omypic.presentation.question.dto;

public class QuestionItemRequest {
    private String text;
    private String questionType;

    public String getText() {
        return text;
    }

    public void setText(String text) {
        this.text = text;
    }

    public String getQuestionType() {
        return questionType;
    }

    public void setQuestionType(String questionType) {
        this.questionType = questionType;
    }
}
