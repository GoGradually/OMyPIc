package me.go_gradually.omypic.application.realtime.model;

public class RealtimeSessionUpdateCommand {
    private String conversationModel;
    private String sttModel;
    private String feedbackModel;
    private String feedbackApiKey;
    private String feedbackLanguage;
    private String ttsVoice;

    public String getConversationModel() {
        return conversationModel;
    }

    public void setConversationModel(String conversationModel) {
        this.conversationModel = conversationModel;
    }

    public String getSttModel() {
        return sttModel;
    }

    public void setSttModel(String sttModel) {
        this.sttModel = sttModel;
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
