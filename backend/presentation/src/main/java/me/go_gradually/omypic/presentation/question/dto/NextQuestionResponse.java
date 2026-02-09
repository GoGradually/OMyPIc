package me.go_gradually.omypic.presentation.question.dto;

public class NextQuestionResponse {
    private String questionId;
    private String text;
    private String group;
    private boolean skipped;

    public static NextQuestionResponse skipped() {
        NextQuestionResponse response = new NextQuestionResponse();
        response.setSkipped(true);
        return response;
    }

    public String getQuestionId() {
        return questionId;
    }

    public void setQuestionId(String questionId) {
        this.questionId = questionId;
    }

    public String getText() {
        return text;
    }

    public void setText(String text) {
        this.text = text;
    }

    public String getGroup() {
        return group;
    }

    public void setGroup(String group) {
        this.group = group;
    }

    public boolean isSkipped() {
        return skipped;
    }

    public void setSkipped(boolean skipped) {
        this.skipped = skipped;
    }
}
