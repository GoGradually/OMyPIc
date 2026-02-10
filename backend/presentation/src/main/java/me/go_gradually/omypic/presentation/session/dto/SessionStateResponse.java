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
    private MockExamStateResponse mockExamState;
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

    public MockExamStateResponse getMockExamState() {
        return mockExamState;
    }

    public void setMockExamState(MockExamStateResponse mockExamState) {
        this.mockExamState = mockExamState;
    }

    public Map<String, Integer> getListIndices() {
        return listIndices;
    }

    public void setListIndices(Map<String, Integer> listIndices) {
        this.listIndices = listIndices;
    }

    public static class MockExamStateResponse {
        private List<String> groupOrder;
        private Map<String, Integer> groupCounts;
        private Map<String, List<String>> remainingQuestions;
        private Map<String, Integer> selectedCounts;
        private int groupIndex;

        public List<String> getGroupOrder() {
            return groupOrder;
        }

        public void setGroupOrder(List<String> groupOrder) {
            this.groupOrder = groupOrder;
        }

        public Map<String, Integer> getGroupCounts() {
            return groupCounts;
        }

        public void setGroupCounts(Map<String, Integer> groupCounts) {
            this.groupCounts = groupCounts;
        }

        public Map<String, List<String>> getRemainingQuestions() {
            return remainingQuestions;
        }

        public void setRemainingQuestions(Map<String, List<String>> remainingQuestions) {
            this.remainingQuestions = remainingQuestions;
        }

        public Map<String, Integer> getSelectedCounts() {
            return selectedCounts;
        }

        public void setSelectedCounts(Map<String, Integer> selectedCounts) {
            this.selectedCounts = selectedCounts;
        }

        public int getGroupIndex() {
            return groupIndex;
        }

        public void setGroupIndex(int groupIndex) {
            this.groupIndex = groupIndex;
        }
    }
}
