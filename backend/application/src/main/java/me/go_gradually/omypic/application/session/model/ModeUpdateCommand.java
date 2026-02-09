package me.go_gradually.omypic.application.session.model;

import me.go_gradually.omypic.domain.session.ModeType;

import java.util.List;
import java.util.Map;

public class ModeUpdateCommand {
    private String sessionId;
    private String listId;
    private ModeType mode;
    private Integer continuousBatchSize;
    private List<String> mockGroupOrder;
    private Map<String, Integer> mockGroupCounts;

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

    public List<String> getMockGroupOrder() {
        return mockGroupOrder;
    }

    public void setMockGroupOrder(List<String> mockGroupOrder) {
        this.mockGroupOrder = mockGroupOrder;
    }

    public Map<String, Integer> getMockGroupCounts() {
        return mockGroupCounts;
    }

    public void setMockGroupCounts(Map<String, Integer> mockGroupCounts) {
        this.mockGroupCounts = mockGroupCounts;
    }
}
