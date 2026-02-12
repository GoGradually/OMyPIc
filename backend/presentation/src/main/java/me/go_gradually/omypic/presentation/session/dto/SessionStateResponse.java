package me.go_gradually.omypic.presentation.session.dto;

import me.go_gradually.omypic.domain.session.ModeType;

import java.util.List;
import java.util.Map;
import java.util.Set;

public class SessionStateResponse {
    private String sessionId;
    private ModeType mode;
    private int continuousBatchSize;
    private int completedGroupCountSinceLastFeedback;
    private Set<String> selectedGroupTags;
    private List<String> candidateGroupOrder;
    private Map<String, Integer> groupQuestionIndices;
    private List<String> sttSegments;
    private String feedbackLanguage;

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

    public int getCompletedGroupCountSinceLastFeedback() {
        return completedGroupCountSinceLastFeedback;
    }

    public void setCompletedGroupCountSinceLastFeedback(int completedGroupCountSinceLastFeedback) {
        this.completedGroupCountSinceLastFeedback = completedGroupCountSinceLastFeedback;
    }

    public Set<String> getSelectedGroupTags() {
        return selectedGroupTags;
    }

    public void setSelectedGroupTags(Set<String> selectedGroupTags) {
        this.selectedGroupTags = selectedGroupTags;
    }

    public List<String> getCandidateGroupOrder() {
        return candidateGroupOrder;
    }

    public void setCandidateGroupOrder(List<String> candidateGroupOrder) {
        this.candidateGroupOrder = candidateGroupOrder;
    }

    public Map<String, Integer> getGroupQuestionIndices() {
        return groupQuestionIndices;
    }

    public void setGroupQuestionIndices(Map<String, Integer> groupQuestionIndices) {
        this.groupQuestionIndices = groupQuestionIndices;
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

}
