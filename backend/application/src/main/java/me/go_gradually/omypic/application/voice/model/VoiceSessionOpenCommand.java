package me.go_gradually.omypic.application.voice.model;

public class VoiceSessionOpenCommand {
    private String sessionId;
    private String apiKey;
    private String feedbackApiKey;
    private String feedbackProvider;
    private Boolean aiEnabled;
    private String feedbackModel;
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

    public String getApiKey() {
        return apiKey;
    }

    public void setApiKey(String apiKey) {
        this.apiKey = apiKey;
    }

    /**
     * API key for the feedback LLM provider. Only needed when it differs from the voice (STT/TTS) key,
     * i.e. when a Claude model is selected.
     */
    public String getFeedbackApiKey() {
        return feedbackApiKey;
    }

    public void setFeedbackApiKey(String feedbackApiKey) {
        this.feedbackApiKey = feedbackApiKey;
    }

    public String getFeedbackProvider() {
        return feedbackProvider;
    }

    public void setFeedbackProvider(String feedbackProvider) {
        this.feedbackProvider = feedbackProvider;
    }

    /**
     * false runs the session with no AI at all: no STT, no TTS, no feedback, and no API key.
     * null is treated as enabled so existing clients keep their behavior.
     */
    public Boolean getAiEnabled() {
        return aiEnabled;
    }

    public void setAiEnabled(Boolean aiEnabled) {
        this.aiEnabled = aiEnabled;
    }

    public String getFeedbackModel() {
        return feedbackModel;
    }

    public void setFeedbackModel(String feedbackModel) {
        this.feedbackModel = feedbackModel;
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
