package me.go_gradually.omypic.application.feedback.model;

import me.go_gradually.omypic.domain.feedback.Feedback;

public class FeedbackResult {
    private boolean generated;
    private Feedback feedback;

    public static FeedbackResult generated(Feedback feedback) {
        FeedbackResult result = new FeedbackResult();
        result.setGenerated(true);
        result.setFeedback(feedback);
        return result;
    }

    public static FeedbackResult skipped() {
        FeedbackResult result = new FeedbackResult();
        result.setGenerated(false);
        return result;
    }

    public boolean isGenerated() {
        return generated;
    }

    public void setGenerated(boolean generated) {
        this.generated = generated;
    }

    public Feedback getFeedback() {
        return feedback;
    }

    public void setFeedback(Feedback feedback) {
        this.feedback = feedback;
    }
}
