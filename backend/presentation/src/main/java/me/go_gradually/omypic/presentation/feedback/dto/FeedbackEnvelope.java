package me.go_gradually.omypic.presentation.feedback.dto;

public class FeedbackEnvelope {
    private boolean generated;
    private FeedbackResponse feedback;

    public static FeedbackEnvelope generated(FeedbackResponse feedback) {
        FeedbackEnvelope envelope = new FeedbackEnvelope();
        envelope.setGenerated(true);
        envelope.setFeedback(feedback);
        return envelope;
    }

    public static FeedbackEnvelope skipped() {
        FeedbackEnvelope envelope = new FeedbackEnvelope();
        envelope.setGenerated(false);
        return envelope;
    }

    public boolean isGenerated() {
        return generated;
    }

    public void setGenerated(boolean generated) {
        this.generated = generated;
    }

    public FeedbackResponse getFeedback() {
        return feedback;
    }

    public void setFeedback(FeedbackResponse feedback) {
        this.feedback = feedback;
    }
}
