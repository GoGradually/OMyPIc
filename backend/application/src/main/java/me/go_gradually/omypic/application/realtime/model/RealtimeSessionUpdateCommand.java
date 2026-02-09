package me.go_gradually.omypic.application.realtime.model;

public class RealtimeSessionUpdateCommand {
    private String feedbackProvider;
    private String feedbackModel;
    private String feedbackApiKey;
    private String feedbackLanguage;
    private String ttsVoice;

    public String getFeedbackProvider() {
        return feedbackProvider;
    }

    public void setFeedbackProvider(String feedbackProvider) {
        this.feedbackProvider = feedbackProvider;
    }

    public String getFeedbackModel() {
        return feedbackModel;
    }

    public void setFeedbackModel(String feedbackModel) {
        this.feedbackModel = feedbackModel;
    }

    public String getFeedbackApiKey() {
        return feedbackApiKey;
    }

    public void setFeedbackApiKey(String feedbackApiKey) {
        this.feedbackApiKey = feedbackApiKey;
    }

    public String getFeedbackLanguage() {
        return feedbackLanguage;
    }

    public void setFeedbackLanguage(String feedbackLanguage) {
        this.feedbackLanguage = feedbackLanguage;
    }

    public String getTtsVoice() {
        return ttsVoice;
    }

    public void setTtsVoice(String ttsVoice) {
        this.ttsVoice = ttsVoice;
    }
}
