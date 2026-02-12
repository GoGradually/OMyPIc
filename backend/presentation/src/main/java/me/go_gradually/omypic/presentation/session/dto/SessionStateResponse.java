package me.go_gradually.omypic.presentation.session.dto;

import me.go_gradually.omypic.domain.session.ModeType;

import java.util.List;
import java.util.Map;

public class SessionStateResponse {
    private String sessionId;
    private ModeType mode;
    private int continuousBatchSize;
    private int answeredSinceLastFeedback;
    private String activeListId;
    private List<String> sttSegments;
    private String feedbackLanguage;
    private Map<String, Integer> listIndices;

    public String getSessionId() {
        return sessionId;
    }

    public void setSessionId(String sessionId) {
        this.sessionId = sessionId;
    }

    public ModeType getMode() {
        return mode;
    }

    public void setMode(ModeType mode) {
        this.mode = mode;
    }

    public int getContinuousBatchSize() {
        return continuousBatchSize;
    }

    public void setContinuousBatchSize(int continuousBatchSize) {
        this.continuousBatchSize = continuousBatchSize;
    }

    public int getAnsweredSinceLastFeedback() {
        return answeredSinceLastFeedback;
    }

    public void setAnsweredSinceLastFeedback(int answeredSinceLastFeedback) {
        this.answeredSinceLastFeedback = answeredSinceLastFeedback;
    }

    public String getActiveListId() {
        return activeListId;
    }

    public void setActiveListId(String activeListId) {
        this.activeListId = activeListId;
    }

    public List<String> getSttSegments() {
        return sttSegments;
    }

    public void setSttSegments(List<String> sttSegments) {
        this.sttSegments = sttSegments;
    }

    public String getFeedbackLanguage() {
        return feedbackLanguage;
    }

    public void setFeedbackLanguage(String feedbackLanguage) {
        this.feedbackLanguage = feedbackLanguage;
    }

    public Map<String, Integer> getListIndices() {
        return listIndices;
    }

    public void setListIndices(Map<String, Integer> listIndices) {
        this.listIndices = listIndices;
    }
}
