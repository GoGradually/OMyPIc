package me.go_gradually.omypic.presentation.question.dto;

public class QuestionItemRequest {
    private String text;
    private String group;

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
}
