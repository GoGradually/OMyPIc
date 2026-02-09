package me.go_gradually.omypic.presentation.feedback.dto;

import jakarta.validation.constraints.NotBlank;

public class MockFinalFeedbackRequest {
    @NotBlank
    private String sessionId;
    @NotBlank
    private String provider;
    @NotBlank
    private String model;
    @NotBlank
    private String feedbackLanguage;

    public String getSessionId() {
        return sessionId;
    }

    public void setSessionId(String sessionId) {
        this.sessionId = sessionId;
    }

    public String getProvider() {
        return provider;
    }

    public void setProvider(String provider) {
        this.provider = provider;
    }

    public String getModel() {
        return model;
    }

    public void setModel(String model) {
        this.model = model;
    }

    public String getFeedbackLanguage() {
        return feedbackLanguage;
    }

    public void setFeedbackLanguage(String feedbackLanguage) {
        this.feedbackLanguage = feedbackLanguage;
    }
}
