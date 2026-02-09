package me.go_gradually.omypic.application.question.model;

public class NextQuestion {
    private String questionId;
    private String text;
    private String group;
    private boolean skipped;
    private boolean mockExamCompleted;
    private String mockExamCompletionReason;

    public static NextQuestion skipped() {
        NextQuestion response = new NextQuestion();
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

    public boolean isMockExamCompleted() {
        return mockExamCompleted;
    }

    public void setMockExamCompleted(boolean mockExamCompleted) {
        this.mockExamCompleted = mockExamCompleted;
    }

    public String getMockExamCompletionReason() {
        return mockExamCompletionReason;
    }

    public void setMockExamCompletionReason(String mockExamCompletionReason) {
        this.mockExamCompletionReason = mockExamCompletionReason;
    }
}
