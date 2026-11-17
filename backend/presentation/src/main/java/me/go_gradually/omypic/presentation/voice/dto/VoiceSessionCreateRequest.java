package me.go_gradually.omypic.presentation.voice.dto;

import jakarta.validation.constraints.NotBlank;

public class VoiceSessionCreateRequest {
    @NotBlank
    private String sessionId;
    private String feedbackModel;
    private String feedbackProvider;
    private Boolean feedbackEnabled;
    private String feedbackLanguage;
    private String sttModel;
    private String ttsModel;
    private String ttsVoice;

    public String getSessionId() {
        return sessionId;
    }

    public void setSessionId(String sessionId) {
        this.sessionId = sessionId;
    }

    public String getFeedbackModel() {
        return feedbackModel;
    }

    public void setFeedbackModel(String feedbackModel) {
        this.feedbackModel = feedbackModel;
    }

    public String getFeedbackProvider() {
        return feedbackProvider;
    }

    public void setFeedbackProvider(String feedbackProvider) {
        this.feedbackProvider = feedbackProvider;
    }

    public Boolean getFeedbackEnabled() {
        return feedbackEnabled;
    }

    public void setFeedbackEnabled(Boolean feedbackEnabled) {
        this.feedbackEnabled = feedbackEnabled;
    }

    public String getFeedbackLanguage() {
        return feedbackLanguage;
    }

    public void setFeedbackLanguage(String feedbackLanguage) {
        this.feedbackLanguage = feedbackLanguage;
    }

    public String getSttModel() {
        return sttModel;
    }

    public void setSttModel(String sttModel) {
        this.sttModel = sttModel;
    }

    public String getTtsModel() {
        return ttsModel;
    }

    public void setTtsModel(String ttsModel) {
        this.ttsModel = ttsModel;
    }

    public String getTtsVoice() {
        return ttsVoice;
    }

    public void setTtsVoice(String ttsVoice) {
        this.ttsVoice = ttsVoice;
    }
}
