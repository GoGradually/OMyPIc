package me.go_gradually.omypic.application.session.model;

import me.go_gradually.omypic.domain.session.ModeType;

import java.util.List;
import java.util.Map;

public class ModeUpdateCommand {
    private String sessionId;
    private String listId;
    private ModeType mode;
    private Integer continuousBatchSize;
    private MockPlan mockPlan;

    public String getSessionId() {
        return sessionId;
    }

    public void setSessionId(String sessionId) {
        this.sessionId = sessionId;
    }

    public String getListId() {
        return listId;
    }

    public void setListId(String listId) {
        this.listId = listId;
    }

    public ModeType getMode() {
        return mode;
    }

    public void setMode(ModeType mode) {
        this.mode = mode;
    }

    public Integer getContinuousBatchSize() {
        return continuousBatchSize;
    }

    public void setContinuousBatchSize(Integer continuousBatchSize) {
        this.continuousBatchSize = continuousBatchSize;
    }

    public MockPlan getMockPlan() {
        return mockPlan;
    }

    public void setMockPlan(MockPlan mockPlan) {
        this.mockPlan = mockPlan;
    }

    public static class MockPlan {
        private List<String> groupOrder;
        private Map<String, Integer> groupCounts;

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
    }
}
